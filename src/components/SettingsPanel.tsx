import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./SettingsPanel.css";

// ─── Types ────────────────────────────────────────────────────────────────────

interface VoiceCommand {
  id: string;
  trigger: string;
  windowsCmd: string;
  linuxCmd: string;
  description: string;
}

interface PluginConfig {
  enabled: boolean;
  autostart: boolean;
  agentName: string;
  port: number;
  commands: VoiceCommand[];
}

const EMPTY_COMMAND: Omit<VoiceCommand, "id"> = {
  trigger: "",
  windowsCmd: "",
  linuxCmd: "",
  description: "",
};

function newId() {
  return crypto.randomUUID();
}

// ─── Subcomponents ────────────────────────────────────────────────────────────

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
        <span className="cmd-trigger-text" title={cmd.trigger}>{cmd.trigger || <em>—</em>}</span>
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

interface EditModalProps {
  cmd: VoiceCommand | null;
  onSave: (cmd: VoiceCommand) => void;
  onClose: () => void;
}

function EditModal({ cmd, onSave, onClose }: EditModalProps) {
  const [form, setForm] = useState<VoiceCommand>(
    cmd ?? { id: newId(), ...EMPTY_COMMAND }
  );

  const set = <K extends keyof VoiceCommand>(key: K, val: VoiceCommand[K]) =>
    setForm((f) => ({ ...f, [key]: val }));

  const handleSave = () => {
    if (!form.trigger.trim()) return;
    onSave({ ...form, trigger: form.trigger.trim() });
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal-title">{cmd ? "Изменить команду" : "Новая команда"}</h3>

        <label className="field-label">Голосовой триггер <span className="hint">(что произносить после имени агента)</span></label>
        <input
          className="field-input"
          placeholder="например: открой проводник"
          value={form.trigger}
          onChange={(e) => set("trigger", e.target.value)}
          autoFocus
        />

        <label className="field-label">Описание <span className="hint">(необязательно)</span></label>
        <input
          className="field-input"
          placeholder="Файловый менеджер"
          value={form.description}
          onChange={(e) => set("description", e.target.value)}
        />

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

        <div className="modal-footer">
          <button className="btn-secondary" onClick={onClose}>Отмена</button>
          <button
            className="btn-primary"
            onClick={handleSave}
            disabled={!form.trigger.trim()}
          >
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
    enabled: true,
    autostart: true,
    agentName: "Вилли",
    port: 8790,
    commands: [],
  });
  const [platform, setPlatform] = useState<"windows" | "linux">("windows");
  const [saved, setSaved] = useState(false);
  const [editTarget, setEditTarget] = useState<VoiceCommand | "new" | null>(null);
  const [testFeedback, setTestFeedback] = useState<string>("");
  const [portWarning, setPortWarning] = useState(false);
  const [originalPort, setOriginalPort] = useState(8790);

  useEffect(() => {
    invoke<PluginConfig>("get_config").then((cfg) => {
      setConfig(cfg);
      setOriginalPort(cfg.port);
    });
    invoke<string>("get_current_platform").then((p) => {
      setPlatform(p === "windows" ? "windows" : "linux");
    });
  }, []);

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

  const openEdit = (cmd: VoiceCommand) => setEditTarget(cmd);
  const openNew = () => setEditTarget("new");

  return (
    <div className="panel">
      {/* ── Header ── */}
      <div className="panel-header">
        <div className="panel-title">
          <span className="logo">🎙</span>
          <span>easySTT Voice Control</span>
        </div>
        <div className="header-toggles">
          <label className="toggle-row" title="Включить / выключить плагин">
            <input
              type="checkbox"
              checked={config.enabled}
              onChange={(e) => setConfig((c) => ({ ...c, enabled: e.target.checked }))}
            />
            <span className="toggle-label">{config.enabled ? "Включён" : "Выключен"}</span>
          </label>
          <label className="toggle-row" title="Запускать автоматически при старте Windows / Linux">
            <input
              type="checkbox"
              checked={config.autostart}
              onChange={(e) => setConfig((c) => ({ ...c, autostart: e.target.checked }))}
            />
            <span className="toggle-label">Автозапуск</span>
          </label>
        </div>
      </div>

      {/* ── Agent settings ── */}
      <section className="section">
        <h2 className="section-title">Агент</h2>
        <div className="inline-fields">
          <div className="field-group">
            <label className="field-label">Имя агента</label>
            <input
              className="field-input"
              placeholder="Вилли"
              value={config.agentName}
              onChange={(e) => setConfig((c) => ({ ...c, agentName: e.target.value }))}
            />
            <span className="field-hint">
              Произносите перед командой: «<em>{config.agentName || "Вилли"}</em>, открой проводник»
            </span>
          </div>
          <div className="field-group field-group--narrow">
            <label className="field-label">Порт HTTP-сервера</label>
            <input
              className="field-input monospace"
              type="number"
              min={1024}
              max={65535}
              value={config.port}
              onChange={(e) => setConfig((c) => ({ ...c, port: Number(e.target.value) }))}
            />
            <span className="field-hint">easySTT → http://127.0.0.1:{config.port}/intercept</span>
          </div>
        </div>
      </section>

      {/* ── Commands ── */}
      <section className="section section--grow">
        <div className="section-header-row">
          <h2 className="section-title">Команды</h2>
          <div className="section-actions">
            <span className="os-badge">{platform === "windows" ? "🪟 Windows" : "🐧 Linux"}</span>
            <button className="btn-primary btn-small" onClick={openNew}>+ Добавить</button>
          </div>
        </div>

        {config.commands.length === 0 ? (
          <div className="empty-state">
            Нет команд. Нажмите «+ Добавить» чтобы создать первую.
          </div>
        ) : (
          <div className="cmd-list">
            <div className="cmd-list-header">
              <span>Триггер</span>
              <span>Команда ({platform === "windows" ? "Win" : "Linux"})</span>
              <span>Описание</span>
              <span />
            </div>
            {config.commands.map((cmd) => (
              <CommandRow
                key={cmd.id}
                cmd={cmd}
                platform={platform}
                onEdit={openEdit}
                onDelete={handleDeleteCmd}
                onTest={handleTest}
              />
            ))}
          </div>
        )}

        {testFeedback && (
          <div className="feedback-toast">{testFeedback}</div>
        )}
      </section>

      {/* ── How to use ── */}
      <section className="section section--info">
        <h2 className="section-title">Как подключить</h2>
        <ol className="how-to">
          <li>В easySTT → <strong>Настройки → 🧩 Плагины</strong> → «+ Добавить плагин»</li>
          <li>Выберите этот исполняемый файл</li>
          <li>Плагин запустится автоматически на порту <code>{config.port}</code></li>
          <li>Говорите: <em>«{config.agentName || "Вилли"}, открой проводник»</em></li>
        </ol>
        <p className="how-to-note">
          Текущий URL для подключения: <code>http://127.0.0.1:{config.port}</code>
        </p>
      </section>

      {/* ── Footer ── */}
      <div className="panel-footer">
        {portWarning && (
          <span className="port-warning">Порт изменён — перезапустите плагин</span>
        )}
        <button className="btn-primary btn-save" onClick={handleSave}>
          {saved ? "✓ Сохранено" : "Сохранить"}
        </button>
      </div>

      {/* ── Edit Modal ── */}
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
