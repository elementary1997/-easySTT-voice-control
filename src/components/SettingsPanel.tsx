import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./SettingsPanel.css";

// ─── Types ────────────────────────────────────────────────────────────────────

interface VoiceCommand {
  id: string;
  trigger: string;
  aliases: string[];
  windowsCmd: string;
  linuxCmd: string;
  description: string;
  category: string;
  closeTrigger: string;
  closeAliases: string[];
  windowsCloseCmd: string;
  linuxCloseCmd: string;
}

interface PluginConfig {
  enabled: boolean;
  autostart: boolean;
  agentName: string;
  port: number;
  commands: VoiceCommand[];
}

const PRESET_CATEGORIES = ["Приложения", "Браузер", "Система", "Настройки", "Утилиты"];
const CATEGORY_ORDER = [...PRESET_CATEGORIES];

const EMPTY_COMMAND: Omit<VoiceCommand, "id"> = {
  trigger: "", aliases: [],
  windowsCmd: "", linuxCmd: "",
  description: "", category: "",
  closeTrigger: "", closeAliases: [],
  windowsCloseCmd: "", linuxCloseCmd: "",
};

function newId() { return crypto.randomUUID(); }

function normalizeTrigger(t: string): string {
  return t.toLowerCase().trim().replace(/\s+/g, " ");
}

// ─── Shared AliasInput ────────────────────────────────────────────────────────

interface AliasInputProps {
  aliases: string[];
  onChange: (a: string[]) => void;
}

function AliasInput({ aliases, onChange }: AliasInputProps) {
  const [input, setInput] = useState("");
  const add = () => {
    const v = input.trim();
    if (v && !aliases.includes(v)) onChange([...aliases, v]);
    setInput("");
  };
  const remove = (i: number) => onChange(aliases.filter((_, idx) => idx !== i));
  return (
    <div className="aliases-row">
      {aliases.map((a, i) => (
        <span key={i} className="alias-tag">
          {a}<button className="alias-remove" onClick={() => remove(i)}>×</button>
        </span>
      ))}
      <input
        className="alias-input"
        placeholder="ещё фраза..."
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); add(); } }}
      />
    </div>
  );
}

// ─── CommandRow ───────────────────────────────────────────────────────────────

interface CommandRowProps {
  cmd: VoiceCommand;
  platform: "windows" | "linux";
  onEdit: (cmd: VoiceCommand) => void;
  onDelete: (id: string) => void;
  onTest: (id: string) => void;
}

function CommandRow({ cmd, platform, onEdit, onDelete, onTest }: CommandRowProps) {
  const platformCmd = platform === "windows" ? cmd.windowsCmd : cmd.linuxCmd;
  return (
    <div className="cmd-row">
      <div className="cmd-trigger">
        <span className="cmd-trigger-open" title={cmd.trigger}>{cmd.trigger || <em>—</em>}</span>
        {cmd.closeTrigger && (
          <span className="cmd-trigger-close" title={cmd.closeTrigger}>{cmd.closeTrigger}</span>
        )}
      </div>
      <div className="cmd-shell" title={platformCmd}>
        <code>{platformCmd || <em>—</em>}</code>
      </div>
      <div className="cmd-desc" title={cmd.description}>{cmd.description}</div>
      <div className="cmd-actions">
        <button className="btn-icon" title="Тест" onClick={() => onTest(cmd.id)}>▶</button>
        <button className="btn-icon" title="Изменить" onClick={() => onEdit(cmd)}>✎</button>
        <button className="btn-icon btn-danger" title="Удалить" onClick={() => onDelete(cmd.id)}>✕</button>
      </div>
    </div>
  );
}

// ─── EditModal ────────────────────────────────────────────────────────────────

interface EditModalProps {
  cmd: VoiceCommand | null;
  onSave: (cmd: VoiceCommand) => void;
  onClose: () => void;
}

function EditModal({ cmd, onSave, onClose }: EditModalProps) {
  const [form, setForm] = useState<VoiceCommand>(cmd ?? { id: newId(), ...EMPTY_COMMAND });
  const [tab, setTab] = useState<"open" | "close">("open");

  const set = <K extends keyof VoiceCommand>(key: K, val: VoiceCommand[K]) =>
    setForm((f) => ({ ...f, [key]: val }));

  const handleSave = () => {
    if (!form.trigger.trim()) return;
    onSave({ ...form, trigger: form.trigger.trim() });
  };

  const hasCloseAction = form.closeTrigger.trim() || form.windowsCloseCmd.trim() || form.linuxCloseCmd.trim();

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal-title">{cmd ? "Изменить команду" : "Новая команда"}</h3>

        {/* Shared fields */}
        <div className="modal-shared-fields">
          <div className="field-group" style={{ flex: 1 }}>
            <label className="field-label">Категория <span className="hint">(необязательно)</span></label>
            <input
              className="field-input"
              list="category-suggestions"
              placeholder="Приложения"
              value={form.category}
              onChange={(e) => set("category", e.target.value)}
            />
            <datalist id="category-suggestions">
              {PRESET_CATEGORIES.map((c) => <option key={c} value={c} />)}
            </datalist>
          </div>
          <div className="field-group" style={{ flex: 2 }}>
            <label className="field-label">Описание <span className="hint">(необязательно)</span></label>
            <input
              className="field-input"
              placeholder="Файловый менеджер"
              value={form.description}
              onChange={(e) => set("description", e.target.value)}
            />
          </div>
        </div>

        {/* Tabs */}
        <div className="modal-tabs">
          <button
            className={`modal-tab ${tab === "open" ? "active" : ""}`}
            onClick={() => setTab("open")}
          >
            ▶ Открыть
          </button>
          <button
            className={`modal-tab tab-close ${tab === "close" ? "active" : ""}`}
            onClick={() => setTab("close")}
          >
            ✕ Закрыть
            {hasCloseAction && tab !== "close" && <span className="tab-badge" />}
          </button>
        </div>

        {/* Tab: Open */}
        {tab === "open" && (
          <>
            <label className="field-label">
              Фраза <span className="hint">(произносить после имени агента)</span>
            </label>
            <input
              className="field-input"
              placeholder="открой проводник"
              value={form.trigger}
              onChange={(e) => set("trigger", e.target.value)}
              autoFocus
            />

            <label className="field-label">Синонимы <span className="hint">(Enter — добавить)</span></label>
            <AliasInput aliases={form.aliases} onChange={(a) => set("aliases", a)} />

            <div className="os-grid">
              <div>
                <label className="field-label os-label">🪟 Windows</label>
                <input
                  className="field-input monospace"
                  placeholder="explorer.exe"
                  value={form.windowsCmd}
                  onChange={(e) => set("windowsCmd", e.target.value)}
                />
              </div>
              <div>
                <label className="field-label os-label">🐧 Linux</label>
                <input
                  className="field-input monospace"
                  placeholder="xdg-open ~"
                  value={form.linuxCmd}
                  onChange={(e) => set("linuxCmd", e.target.value)}
                />
              </div>
            </div>
          </>
        )}

        {/* Tab: Close */}
        {tab === "close" && (
          <>
            <label className="field-label">
              Фраза для закрытия <span className="hint">(необязательно)</span>
            </label>
            <input
              className="field-input"
              placeholder="закрой проводник"
              value={form.closeTrigger}
              onChange={(e) => set("closeTrigger", e.target.value)}
              autoFocus
            />

            <label className="field-label">Синонимы <span className="hint">(Enter — добавить)</span></label>
            <AliasInput aliases={form.closeAliases} onChange={(a) => set("closeAliases", a)} />

            <div className="os-grid">
              <div>
                <label className="field-label os-label">🪟 Windows</label>
                <input
                  className="field-input monospace"
                  placeholder="taskkill /f /im explorer.exe"
                  value={form.windowsCloseCmd}
                  onChange={(e) => set("windowsCloseCmd", e.target.value)}
                />
              </div>
              <div>
                <label className="field-label os-label">🐧 Linux</label>
                <input
                  className="field-input monospace"
                  placeholder="pkill nautilus"
                  value={form.linuxCloseCmd}
                  onChange={(e) => set("linuxCloseCmd", e.target.value)}
                />
              </div>
            </div>

            {!hasCloseAction && (
              <p className="close-tab-hint">
                Оставьте пустым — тогда команда закрытия работать не будет
              </p>
            )}
          </>
        )}

        <div className="modal-footer">
          <button className="btn-secondary" onClick={onClose}>Отмена</button>
          <button className="btn-primary" onClick={handleSave} disabled={!form.trigger.trim()}>
            Сохранить
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Main Panel ───────────────────────────────────────────────────────────────

export default function SettingsPanel() {
  const [config, setConfig] = useState<PluginConfig>({
    enabled: true, autostart: true,
    agentName: "Вилли", port: 8790, commands: [],
  });
  const [platform, setPlatform] = useState<"windows" | "linux">("windows");
  const [saved, setSaved] = useState(false);
  const [editTarget, setEditTarget] = useState<VoiceCommand | "new" | null>(null);
  const [testFeedback, setTestFeedback] = useState<string>("");
  const [portWarning, setPortWarning] = useState(false);
  const [originalPort, setOriginalPort] = useState(8790);
  const importRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    invoke<PluginConfig>("get_config").then((cfg) => {
      setConfig(cfg);
      setOriginalPort(cfg.port);
    });
    invoke<string>("get_current_platform").then((p) => {
      setPlatform(p === "windows" ? "windows" : "linux");
    });
  }, []);

  // ── Grouped commands ──────────────────────────────────────────────────────

  const grouped = useMemo(() => {
    const map: Record<string, VoiceCommand[]> = {};
    for (const cmd of config.commands) {
      const cat = cmd.category?.trim() || "Другое";
      if (!map[cat]) map[cat] = [];
      map[cat].push(cmd);
    }
    return map;
  }, [config.commands]);

  const sortedCats = useMemo(() => {
    return Object.keys(grouped).sort((a, b) => {
      const ia = CATEGORY_ORDER.indexOf(a);
      const ib = CATEGORY_ORDER.indexOf(b);
      if (ia !== -1 && ib !== -1) return ia - ib;
      if (ia !== -1) return -1;
      if (ib !== -1) return 1;
      if (a === "Другое") return 1;
      if (b === "Другое") return -1;
      return a.localeCompare(b, "ru");
    });
  }, [grouped]);

  // ── Actions ───────────────────────────────────────────────────────────────

  const handleSave = useCallback(async () => {
    await invoke("save_config", { config });
    setSaved(true);
    setPortWarning(config.port !== originalPort);
    setTimeout(() => setSaved(false), 2500);
  }, [config, originalPort]);

  const handleTest = useCallback(async (id: string) => {
    try {
      const msg = await invoke<string>("test_command", { commandId: id });
      setTestFeedback(msg);
    } catch (e) {
      setTestFeedback(`Ошибка: ${e}`);
    }
    setTimeout(() => setTestFeedback(""), 3000);
  }, []);

  const handleDeleteCmd = useCallback((id: string) => {
    if (!confirm("Удалить команду?")) return;
    setConfig((c) => ({ ...c, commands: c.commands.filter((cmd) => cmd.id !== id) }));
  }, []);

  const handleEditSave = useCallback((cmd: VoiceCommand) => {
    setConfig((c) => {
      const exists = c.commands.some((x) => x.id === cmd.id);
      return {
        ...c,
        commands: exists
          ? c.commands.map((x) => (x.id === cmd.id ? cmd : x))
          : [...c.commands, cmd],
      };
    });
    setEditTarget(null);
  }, []);

  // ── Export / Import ───────────────────────────────────────────────────────

  const handleExport = useCallback(() => {
    const blob = new Blob([JSON.stringify(config.commands, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = "voice-commands.json"; a.click();
    URL.revokeObjectURL(url);
  }, [config.commands]);

  const handleImport = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      try {
        const raw = JSON.parse(ev.target?.result as string);
        const incoming: VoiceCommand[] = Array.isArray(raw) ? raw : [];
        const existingKeys = new Set(
          config.commands.flatMap((c) => [
            normalizeTrigger(c.trigger),
            ...c.aliases.map(normalizeTrigger),
          ])
        );
        let added = 0, skipped = 0;
        const toAdd: VoiceCommand[] = [];
        for (const cmd of incoming) {
          if (!cmd.trigger?.trim()) { skipped++; continue; }
          const norm = normalizeTrigger(cmd.trigger);
          if (existingKeys.has(norm)) { skipped++; continue; }
          toAdd.push({ ...EMPTY_COMMAND, ...cmd, id: newId() });
          existingKeys.add(norm);
          added++;
        }
        if (toAdd.length > 0) setConfig((c) => ({ ...c, commands: [...c.commands, ...toAdd] }));
        const msg = added > 0
          ? `Добавлено: ${added}${skipped > 0 ? `, пропущено дублей: ${skipped}` : ""}`
          : `Все команды уже есть (пропущено: ${skipped})`;
        setTestFeedback(msg);
        setTimeout(() => setTestFeedback(""), 3500);
      } catch {
        setTestFeedback("Ошибка: не удалось разобрать JSON");
        setTimeout(() => setTestFeedback(""), 3000);
      }
      e.target.value = "";
    };
    reader.readAsText(file);
  }, [config.commands]);

  // ─────────────────────────────────────────────────────────────────────────

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <span className="logo">🎙</span>
          <span>easySTT Voice Control</span>
        </div>
        <div className="header-toggles">
          <label className="toggle-row" title="Включить / выключить плагин">
            <input type="checkbox" checked={config.enabled}
              onChange={(e) => setConfig((c) => ({ ...c, enabled: e.target.checked }))} />
            <span className="toggle-label">{config.enabled ? "Включён" : "Выключен"}</span>
          </label>
          <label className="toggle-row" title="Запускать автоматически при старте">
            <input type="checkbox" checked={config.autostart}
              onChange={(e) => setConfig((c) => ({ ...c, autostart: e.target.checked }))} />
            <span className="toggle-label">Автозапуск</span>
          </label>
        </div>
      </div>

      <section className="section">
        <h2 className="section-title">Агент</h2>
        <div className="inline-fields">
          <div className="field-group">
            <label className="field-label">Имя агента</label>
            <input className="field-input" placeholder="Вилли" value={config.agentName}
              onChange={(e) => setConfig((c) => ({ ...c, agentName: e.target.value }))} />
            <span className="field-hint">
              Произносите перед командой: «<em>{config.agentName || "Вилли"}</em>, открой проводник»
            </span>
          </div>
          <div className="field-group field-group--narrow">
            <label className="field-label">Порт HTTP-сервера</label>
            <input className="field-input monospace" type="number" min={1024} max={65535}
              value={config.port}
              onChange={(e) => setConfig((c) => ({ ...c, port: Number(e.target.value) }))} />
            <span className="field-hint">easySTT → http://127.0.0.1:{config.port}/intercept</span>
          </div>
        </div>
      </section>

      <section className="section section--grow">
        <div className="section-header-row">
          <h2 className="section-title">Команды</h2>
          <div className="section-actions">
            <span className="os-badge">{platform === "windows" ? "🪟 Windows" : "🐧 Linux"}</span>
            <button className="btn-secondary btn-small" onClick={handleExport}
              disabled={config.commands.length === 0}>Экспорт</button>
            <button className="btn-secondary btn-small" onClick={() => importRef.current?.click()}>Импорт</button>
            <input ref={importRef} type="file" accept=".json" style={{ display: "none" }} onChange={handleImport} />
            <button className="btn-primary btn-small" onClick={() => setEditTarget("new")}>+ Добавить</button>
          </div>
        </div>

        {config.commands.length === 0 ? (
          <div className="empty-state">Нет команд. Нажмите «+ Добавить» чтобы создать первую.</div>
        ) : (
          <div className="cmd-list">
            <div className="cmd-list-header">
              <span>Триггер</span>
              <span>Команда ({platform === "windows" ? "Win" : "Linux"})</span>
              <span>Описание</span>
              <span />
            </div>
            {sortedCats.map((cat) => (
              <div key={cat} className="cmd-category-group">
                <div className="cmd-category-header">{cat}</div>
                {grouped[cat].map((cmd) => (
                  <CommandRow key={cmd.id} cmd={cmd} platform={platform}
                    onEdit={(c) => setEditTarget(c)}
                    onDelete={handleDeleteCmd}
                    onTest={handleTest} />
                ))}
              </div>
            ))}
          </div>
        )}

        {testFeedback && <div className="feedback-toast">{testFeedback}</div>}
      </section>

      <div className="panel-footer">
        {portWarning && <span className="port-warning">Порт изменён — перезапустите плагин</span>}
        <button className="btn-primary btn-save" onClick={handleSave}>
          {saved ? "✓ Сохранено" : "Сохранить"}
        </button>
      </div>

      {editTarget !== null && (
        <EditModal
          cmd={editTarget === "new" ? null : editTarget}
          onSave={handleEditSave}
          onClose={() => setEditTarget(null)}
        />
      )}
    </div>
  );
}
