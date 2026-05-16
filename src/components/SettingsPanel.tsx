import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  categories: string[];
  ollamaEnabled: boolean;
  ollamaUrl: string;
  ollamaModel: string;
  voiceFeedbackEnabled: boolean;
  voiceFeedbackStyle: string;
  voiceEngine: string;
  piperVoice: string;
  voiceCustomCmd: string;
}

interface PiperVoice {
  id: string;
  displayName: string;
  gender: string;
  sizeMb: number;
  installed: boolean;
  hfPath: string;
}

interface PiperStatus {
  binaryInstalled: boolean;
  voices: PiperVoice[];
}

const DEFAULT_CONFIG: PluginConfig = {
  enabled: true, autostart: true,
  agentName: "Вилли", port: 8790,
  commands: [], categories: [],
  ollamaEnabled: false,
  ollamaUrl: "http://localhost:1234",
  ollamaModel: "",
  voiceFeedbackEnabled: false,
  voiceFeedbackStyle: "neutral",
  voiceEngine: "system",
  piperVoice: "ru_RU-irina-medium",
  voiceCustomCmd: "",
};

const EMPTY_COMMAND: Omit<VoiceCommand, "id"> = {
  trigger: "", aliases: [],
  windowsCmd: "", linuxCmd: "",
  description: "", category: "",
  closeTrigger: "", closeAliases: [],
  windowsCloseCmd: "", linuxCloseCmd: "",
};

function newId() { return crypto.randomUUID(); }
function normalizeTrigger(t: string) { return t.toLowerCase().trim().replace(/\s+/g, " "); }

// ─── AliasInput ───────────────────────────────────────────────────────────────

function AliasInput({ aliases, onChange }: { aliases: string[]; onChange: (a: string[]) => void }) {
  const [input, setInput] = useState("");
  const add = () => { const v = input.trim(); if (v && !aliases.includes(v)) onChange([...aliases, v]); setInput(""); };
  return (
    <div className="aliases-row">
      {aliases.map((a, i) => (
        <span key={i} className="alias-tag">
          {a}<button className="alias-remove" onClick={() => onChange(aliases.filter((_, j) => j !== i))}>×</button>
        </span>
      ))}
      <input className="alias-input" placeholder="ещё фраза..." value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); add(); } }} />
    </div>
  );
}

// ─── CommandRow ───────────────────────────────────────────────────────────────

function CommandRow({ cmd, platform, onEdit, onDelete, onTest }: {
  cmd: VoiceCommand; platform: "windows" | "linux";
  onEdit: (c: VoiceCommand) => void; onDelete: (id: string) => void; onTest: (id: string) => void;
}) {
  const exec = platform === "windows" ? cmd.windowsCmd : cmd.linuxCmd;
  return (
    <div className="cmd-row">
      <div className="cmd-trigger">
        <span className="cmd-trigger-open" title={cmd.trigger}>{cmd.trigger || <em>—</em>}</span>
        {cmd.closeTrigger && <span className="cmd-trigger-close" title={cmd.closeTrigger}>{cmd.closeTrigger}</span>}
      </div>
      <div className="cmd-shell" title={exec}><code>{exec || <em>—</em>}</code></div>
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

function EditModal({ cmd, categories, onSave, onClose }: {
  cmd: VoiceCommand | null; categories: string[];
  onSave: (c: VoiceCommand) => void; onClose: () => void;
}) {
  const [form, setForm] = useState<VoiceCommand>(cmd ?? { id: newId(), ...EMPTY_COMMAND });
  const [tab, setTab] = useState<"open" | "close">("open");
  const set = <K extends keyof VoiceCommand>(k: K, v: VoiceCommand[K]) => setForm(f => ({ ...f, [k]: v }));
  const hasClose = form.closeTrigger.trim() || form.windowsCloseCmd.trim() || form.linuxCloseCmd.trim();

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3 className="modal-title">{cmd ? "Изменить команду" : "Новая команда"}</h3>
        <div className="modal-shared-fields">
          <div className="field-group" style={{ flex: 1 }}>
            <label className="field-label">Категория <span className="hint">(необязательно)</span></label>
            <input className="field-input" list="cat-suggestions" placeholder="Приложения"
              value={form.category} onChange={e => set("category", e.target.value)} />
            <datalist id="cat-suggestions">{categories.map(c => <option key={c} value={c} />)}</datalist>
          </div>
          <div className="field-group" style={{ flex: 2 }}>
            <label className="field-label">Описание <span className="hint">(необязательно)</span></label>
            <input className="field-input" placeholder="Файловый менеджер"
              value={form.description} onChange={e => set("description", e.target.value)} />
          </div>
        </div>
        <div className="modal-tabs">
          <button className={`modal-tab ${tab === "open" ? "active" : ""}`} onClick={() => setTab("open")}>▶ Открыть</button>
          <button className={`modal-tab tab-close ${tab === "close" ? "active" : ""}`} onClick={() => setTab("close")}>
            ✕ Закрыть {hasClose && tab !== "close" && <span className="tab-badge" />}
          </button>
        </div>
        {tab === "open" && <>
          <label className="field-label">Фраза <span className="hint">(произносить после имени агента)</span></label>
          <input className="field-input" placeholder="открой проводник" value={form.trigger}
            onChange={e => set("trigger", e.target.value)} autoFocus />
          <label className="field-label">Синонимы <span className="hint">(Enter — добавить)</span></label>
          <AliasInput aliases={form.aliases} onChange={a => set("aliases", a)} />
          <div className="os-grid">
            <div><label className="field-label os-label">🪟 Windows</label>
              <input className="field-input monospace" placeholder="explorer.exe" value={form.windowsCmd}
                onChange={e => set("windowsCmd", e.target.value)} /></div>
            <div><label className="field-label os-label">🐧 Linux</label>
              <input className="field-input monospace" placeholder="xdg-open ~" value={form.linuxCmd}
                onChange={e => set("linuxCmd", e.target.value)} /></div>
          </div>
        </>}
        {tab === "close" && <>
          <label className="field-label">Фраза для закрытия <span className="hint">(необязательно)</span></label>
          <input className="field-input" placeholder="закрой проводник" value={form.closeTrigger}
            onChange={e => set("closeTrigger", e.target.value)} autoFocus />
          <label className="field-label">Синонимы <span className="hint">(Enter — добавить)</span></label>
          <AliasInput aliases={form.closeAliases} onChange={a => set("closeAliases", a)} />
          <div className="os-grid">
            <div><label className="field-label os-label">🪟 Windows</label>
              <input className="field-input monospace" placeholder="taskkill /f /im explorer.exe" value={form.windowsCloseCmd}
                onChange={e => set("windowsCloseCmd", e.target.value)} /></div>
            <div><label className="field-label os-label">🐧 Linux</label>
              <input className="field-input monospace" placeholder="pkill nautilus" value={form.linuxCloseCmd}
                onChange={e => set("linuxCloseCmd", e.target.value)} /></div>
          </div>
          {!hasClose && <p className="close-tab-hint">Оставьте пустым — команда закрытия работать не будет</p>}
        </>}
        <div className="modal-footer">
          <button className="btn-secondary" onClick={onClose}>Отмена</button>
          <button className="btn-primary" onClick={() => { if (form.trigger.trim()) onSave({ ...form, trigger: form.trigger.trim() }); }}
            disabled={!form.trigger.trim()}>Сохранить</button>
        </div>
      </div>
    </div>
  );
}

// ─── Main Panel ───────────────────────────────────────────────────────────────

type MainTab = "commands" | "ai";

export default function SettingsPanel() {
  const [config, setConfig] = useState<PluginConfig>(DEFAULT_CONFIG);
  const [platform, setPlatform] = useState<"windows" | "linux">("windows");
  const [saved, setSaved] = useState(false);
  const [editTarget, setEditTarget] = useState<VoiceCommand | "new" | null>(null);
  const [feedback, setFeedback] = useState("");
  const [portWarning, setPortWarning] = useState(false);
  const [originalPort, setOriginalPort] = useState(8790);
  const [mainTab, setMainTab] = useState<MainTab>("commands");

  // Commands tab state
  const [activeCategory, setActiveCategory] = useState("Все");
  const [addingCat, setAddingCat] = useState(false);
  const [newCatInput, setNewCatInput] = useState("");
  const importRef = useRef<HTMLInputElement>(null);

  // AI tab state
  const [aiOnline, setAiOnline] = useState<boolean | null>(null);
  const [modelList, setModelList] = useState<string[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [ttsTestText, setTtsTestText] = useState("Привет! Я готов к работе.");

  // Piper state
  const [piperStatus, setPiperStatus] = useState<PiperStatus>({ binaryInstalled: false, voices: [] });
  const [piperProgress, setPiperProgress] = useState<Record<string, number>>({});
  const [piperDownloading, setPiperDownloading] = useState<string | null>(null);

  // ── Init ──────────────────────────────────────────────────────────────────

  useEffect(() => {
    invoke<PluginConfig>("get_config").then(cfg => { setConfig(cfg); setOriginalPort(cfg.port); });
    invoke<string>("get_current_platform").then(p => setPlatform(p === "windows" ? "windows" : "linux"));
  }, []);

  // Piper download events
  useEffect(() => {
    const u1 = listen<{ kind: string; id: string; pct: number }>("piper-progress", ({ payload }) => {
      setPiperProgress(p => ({ ...p, [payload.id]: payload.pct }));
    });
    const u2 = listen<{ kind: string; id: string }>("piper-done", ({ payload }) => {
      setPiperDownloading(null);
      setPiperProgress(p => { const n = { ...p }; delete n[payload.id]; return n; });
      invoke<PiperStatus>("get_piper_status").then(setPiperStatus);
      showFeedback(payload.id === "binary" ? "Piper установлен!" : "Голос загружен!");
    });
    const u3 = listen<{ kind: string; id: string; error: string }>("piper-error", ({ payload }) => {
      setPiperDownloading(null);
      setPiperProgress(p => { const n = { ...p }; delete n[payload.id]; return n; });
      showFeedback(`Ошибка: ${payload.error}`);
    });
    return () => { u1.then(f => f()); u2.then(f => f()); u3.then(f => f()); };
  }, []);

  // ── Helpers ───────────────────────────────────────────────────────────────

  const showFeedback = (msg: string, ms = 3500) => {
    setFeedback(msg);
    setTimeout(() => setFeedback(""), ms);
  };

  const loadModels = useCallback(async () => {
    setCatalogLoading(true);
    try {
      const online = await invoke<boolean>("check_ollama", { url: config.ollamaUrl });
      setAiOnline(online);
      if (online) {
        const models = await invoke<string[]>("get_ai_models");
        setModelList(models);
      } else {
        setModelList([]);
      }
    } finally {
      setCatalogLoading(false);
    }
  }, [config.ollamaUrl]);

  useEffect(() => {
    if (mainTab === "ai") loadModels();
  }, [mainTab, loadModels]);

  // ── Category helpers ──────────────────────────────────────────────────────

  const orphanCommands = useMemo(() =>
    config.commands.filter(c => !c.category.trim() || !config.categories.includes(c.category)),
    [config.commands, config.categories]
  );

  const filteredCommands = useMemo(() => {
    if (activeCategory === "Все") return config.commands;
    if (activeCategory === "Другое") return orphanCommands;
    return config.commands.filter(c => c.category === activeCategory);
  }, [config.commands, activeCategory, orphanCommands]);

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
      showFeedback(msg);
    } catch (e) { showFeedback(`Ошибка: ${e}`); }
  }, []);

  const handleDeleteCmd = useCallback((id: string) => {
    if (!confirm("Удалить команду?")) return;
    setConfig(c => ({ ...c, commands: c.commands.filter(cmd => cmd.id !== id) }));
  }, []);

  const handleEditSave = useCallback((cmd: VoiceCommand) => {
    setConfig(c => {
      const exists = c.commands.some(x => x.id === cmd.id);
      const cat = cmd.category.trim();
      const cats = cat && !c.categories.includes(cat) ? [...c.categories, cat] : c.categories;
      return { ...c, categories: cats, commands: exists ? c.commands.map(x => x.id === cmd.id ? cmd : x) : [...c.commands, cmd] };
    });
    setEditTarget(null);
  }, []);

  const handleAddCategory = useCallback(() => {
    const name = newCatInput.trim();
    if (name && !config.categories.includes(name)) {
      setConfig(c => ({ ...c, categories: [...c.categories, name] }));
      setActiveCategory(name);
    }
    setAddingCat(false); setNewCatInput("");
  }, [newCatInput, config.categories]);

  const handleRemoveCategory = useCallback((cat: string) => {
    if (!confirm(`Удалить категорию «${cat}»? Команды перейдут в «Другое».`)) return;
    setConfig(c => ({
      ...c,
      categories: c.categories.filter(x => x !== cat),
      commands: c.commands.map(cmd => cmd.category === cat ? { ...cmd, category: "" } : cmd),
    }));
    if (activeCategory === cat) setActiveCategory("Все");
  }, [activeCategory]);

  const handleExport = useCallback(async () => {
    try { const path = await invoke<string>("export_commands"); showFeedback(`Сохранено: ${path}`, 4000); }
    catch (e) { showFeedback(`Ошибка: ${e}`); }
  }, []);

  const handleImport = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]; if (!file) return;
    const reader = new FileReader();
    reader.onload = ev => {
      try {
        const raw = JSON.parse(ev.target?.result as string);
        const incoming: VoiceCommand[] = Array.isArray(raw) ? raw : [];
        const existingKeys = new Set(config.commands.flatMap(c => [normalizeTrigger(c.trigger), ...c.aliases.map(normalizeTrigger)]));
        let added = 0, skipped = 0;
        const toAdd: VoiceCommand[] = [];
        const newCats: string[] = [];
        for (const cmd of incoming) {
          if (!cmd.trigger?.trim()) { skipped++; continue; }
          const norm = normalizeTrigger(cmd.trigger);
          if (existingKeys.has(norm)) { skipped++; continue; }
          toAdd.push({ ...EMPTY_COMMAND, ...cmd, id: newId() });
          existingKeys.add(norm); added++;
          const cat = cmd.category?.trim();
          if (cat && !newCats.includes(cat)) newCats.push(cat);
        }
        if (toAdd.length > 0) setConfig(c => {
          const merged = [...c.categories, ...newCats.filter(cat => !c.categories.includes(cat))];
          return { ...c, categories: merged, commands: [...c.commands, ...toAdd] };
        });
        showFeedback(added > 0 ? `Добавлено: ${added}${skipped > 0 ? `, пропущено: ${skipped}` : ""}` : `Все команды уже есть (${skipped})`);
      } catch { showFeedback("Ошибка разбора JSON"); }
      e.target.value = "";
    };
    reader.readAsText(file);
  }, [config.commands]);

  const handleTestTts = useCallback(() => {
    invoke("test_tts", { text: ttsTestText }).catch(e => showFeedback(`TTS ошибка: ${e}`));
  }, [ttsTestText]);

  const loadPiperStatus = useCallback(() => {
    invoke<PiperStatus>("get_piper_status").then(setPiperStatus);
  }, []);

  useEffect(() => {
    if (mainTab === "ai") loadPiperStatus();
  }, [mainTab, loadPiperStatus]);

  const handleDownloadBinary = useCallback(() => {
    setPiperDownloading("binary");
    invoke("download_piper_binary");
  }, []);

  const handleDownloadVoice = useCallback((voiceId: string) => {
    setPiperDownloading(voiceId);
    invoke("download_piper_voice", { voiceId });
  }, []);

  const handleCancelPiper = useCallback(() => {
    invoke("cancel_piper_download");
    setPiperDownloading(null);
  }, []);

  // ─────────────────────────────────────────────────────────────────────────

  return (
    <div className="panel">

      {/* ── Header ── */}
      <div className="panel-header">
        <div className="panel-title">
          <span className="logo">🎙</span>
          <span>easySTT Voice Control</span>
        </div>
        <div className="header-toggles">
          <label className="toggle-row">
            <input type="checkbox" checked={config.enabled}
              onChange={e => setConfig(c => ({ ...c, enabled: e.target.checked }))} />
            <span className="toggle-label">{config.enabled ? "Включён" : "Выключен"}</span>
          </label>
          <label className="toggle-row">
            <input type="checkbox" checked={config.autostart}
              onChange={e => setConfig(c => ({ ...c, autostart: e.target.checked }))} />
            <span className="toggle-label">Автозапуск</span>
          </label>
        </div>
      </div>

      {/* ── Agent ── */}
      <section className="section">
        <h2 className="section-title">Агент</h2>
        <div className="inline-fields">
          <div className="field-group">
            <label className="field-label">Имя агента</label>
            <input className="field-input" placeholder="Вилли" value={config.agentName}
              onChange={e => setConfig(c => ({ ...c, agentName: e.target.value }))} />
            <span className="field-hint">«<em>{config.agentName || "Вилли"}</em>, открой проводник»</span>
          </div>
          <div className="field-group field-group--narrow">
            <label className="field-label">Порт</label>
            <input className="field-input monospace" type="number" min={1024} max={65535}
              value={config.port} onChange={e => setConfig(c => ({ ...c, port: Number(e.target.value) }))} />
            <span className="field-hint">:{config.port}/intercept</span>
          </div>
        </div>
      </section>

      {/* ── Main tabs ── */}
      <div className="main-tabs">
        <button className={`main-tab ${mainTab === "commands" ? "main-tab--active" : ""}`}
          onClick={() => setMainTab("commands")}>Команды</button>
        <button className={`main-tab ${mainTab === "ai" ? "main-tab--active" : ""}`}
          onClick={() => setMainTab("ai")}>
          AI Ассистент
          {config.ollamaEnabled && <span className="main-tab-dot" />}
        </button>
      </div>

      {/* ═══════════════ COMMANDS TAB ═══════════════ */}
      {mainTab === "commands" && (
        <section className="section section--grow">
          <div className="section-header-row">
            <h2 className="section-title">Команды</h2>
            <div className="section-actions">
              <span className="os-badge">{platform === "windows" ? "🪟 Windows" : "🐧 Linux"}</span>
              <button className="btn-secondary btn-small" onClick={handleExport} disabled={config.commands.length === 0}>Экспорт</button>
              <button className="btn-secondary btn-small" onClick={() => importRef.current?.click()}>Импорт</button>
              <input ref={importRef} type="file" accept=".json" style={{ display: "none" }} onChange={handleImport} />
              <button className="btn-primary btn-small" onClick={() => setEditTarget("new")}>+ Добавить</button>
            </div>
          </div>

          {/* Category tabs */}
          <div className="cat-tabs">
            <button className={`cat-tab ${activeCategory === "Все" ? "cat-tab--active" : ""}`} onClick={() => setActiveCategory("Все")}>
              Все <span className="cat-tab-count">{config.commands.length}</span>
            </button>
            {config.categories.map(cat => (
              <button key={cat} className={`cat-tab ${activeCategory === cat ? "cat-tab--active" : ""}`} onClick={() => setActiveCategory(cat)}>
                {cat}
                <span className="cat-tab-count">{config.commands.filter(c => c.category === cat).length}</span>
                <span className="cat-tab-remove" title={`Удалить «${cat}»`}
                  onClick={e => { e.stopPropagation(); handleRemoveCategory(cat); }}>×</span>
              </button>
            ))}
            {orphanCommands.length > 0 && (
              <button className={`cat-tab ${activeCategory === "Другое" ? "cat-tab--active" : ""}`} onClick={() => setActiveCategory("Другое")}>
                Другое <span className="cat-tab-count">{orphanCommands.length}</span>
              </button>
            )}
            {addingCat ? (
              <div className="cat-tab-add">
                <input className="cat-tab-input" placeholder="Название..." value={newCatInput} autoFocus
                  onChange={e => setNewCatInput(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") handleAddCategory(); if (e.key === "Escape") { setAddingCat(false); setNewCatInput(""); } }}
                  onBlur={handleAddCategory} />
              </div>
            ) : (
              <button className="cat-tab cat-tab--add" onClick={() => setAddingCat(true)}>+ Категория</button>
            )}
          </div>

          {config.commands.length === 0 ? (
            <div className="empty-state">Нет команд. Нажмите «+ Добавить».</div>
          ) : filteredCommands.length === 0 ? (
            <div className="empty-state">В «{activeCategory}» нет команд.</div>
          ) : (
            <div className="cmd-list">
              <div className="cmd-list-header">
                <span>Триггер</span>
                <span>Команда ({platform === "windows" ? "Win" : "Linux"})</span>
                <span>Описание</span>
                <span />
              </div>
              {filteredCommands.map(cmd => (
                <CommandRow key={cmd.id} cmd={cmd} platform={platform}
                  onEdit={c => setEditTarget(c)} onDelete={handleDeleteCmd} onTest={handleTest} />
              ))}
            </div>
          )}

          {feedback && <div className="feedback-toast">{feedback}</div>}
        </section>
      )}

      {/* ═══════════════ AI TAB ═══════════════ */}
      {mainTab === "ai" && (
        <section className="section section--grow ai-section">

          {/* LM Studio NLU */}
          <div className="ai-block">
            <div className="ai-block-header">
              <div>
                <h2 className="section-title" style={{ marginBottom: 2 }}>LM Studio NLU</h2>
                <span className="field-hint">Умное распознавание команд через локальную модель</span>
              </div>
              <label className="toggle-row">
                <input type="checkbox" checked={config.ollamaEnabled}
                  onChange={e => setConfig(c => ({ ...c, ollamaEnabled: e.target.checked }))} />
                <span className="toggle-label">{config.ollamaEnabled ? "Вкл" : "Выкл"}</span>
              </label>
            </div>

            {config.ollamaEnabled && <>
              <div className="ai-url-row">
                <div className="field-group" style={{ flex: 1 }}>
                  <label className="field-label">URL сервера</label>
                  <input className="field-input monospace" value={config.ollamaUrl}
                    onChange={e => setConfig(c => ({ ...c, ollamaUrl: e.target.value }))} />
                </div>
                <button className="btn-secondary btn-small ai-check-btn" onClick={loadModels} disabled={catalogLoading}>
                  {catalogLoading ? "Проверяю..." : "Проверить"}
                </button>
              </div>

              {aiOnline !== null && (
                <div className={`ollama-status ${aiOnline ? "ollama-status--online" : "ollama-status--offline"}`}>
                  {aiOnline
                    ? `● Сервер доступен${modelList.length > 0 ? ` · ${modelList.length} мод.` : ""}`
                    : "● Сервер недоступен — запустите LM Studio и включите Local Server"}
                </div>
              )}

              {aiOnline && (
                <>
                  <label className="field-label" style={{ marginTop: 10 }}>Модель</label>
                  {modelList.length > 0 ? (
                    <select className="field-input" value={config.ollamaModel}
                      onChange={e => setConfig(c => ({ ...c, ollamaModel: e.target.value }))}>
                      <option value="">— выберите модель —</option>
                      {modelList.map(m => (
                        <option key={m} value={m} title={m}>
                          {m.length > 60 ? `…${m.slice(-58)}` : m}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <p className="ai-no-models">Нет загруженных моделей. Загрузите модель в LM Studio.</p>
                  )}
                  {config.ollamaModel && (
                    <span className="field-hint" style={{ marginTop: 2 }}>
                      Активная: <em>{config.ollamaModel}</em>
                    </span>
                  )}
                </>
              )}
            </>}
          </div>

          {/* Voice feedback */}
          <div className="ai-block">
            <div className="ai-block-header">
              <div>
                <h2 className="section-title" style={{ marginBottom: 2 }}>Голосовой ответ</h2>
                <span className="field-hint">Ассистент произносит ответ после команды</span>
              </div>
              <label className="toggle-row">
                <input type="checkbox" checked={config.voiceFeedbackEnabled}
                  onChange={e => setConfig(c => ({ ...c, voiceFeedbackEnabled: e.target.checked }))} />
                <span className="toggle-label">{config.voiceFeedbackEnabled ? "Вкл" : "Выкл"}</span>
              </label>
            </div>

            {config.voiceFeedbackEnabled && (
              <>
                <label className="field-label" style={{ marginTop: 8 }}>Стиль ответа</label>
                <div className="style-selector">
                  <label className={`style-option ${config.voiceFeedbackStyle === "neutral" ? "style-option--active" : ""}`}>
                    <input type="radio" name="style" value="neutral" checked={config.voiceFeedbackStyle === "neutral"}
                      onChange={() => setConfig(c => ({ ...c, voiceFeedbackStyle: "neutral" }))} />
                    <span className="style-label">Нейтральный</span>
                    <span className="style-example">«Выполняю», «Готово»</span>
                  </label>
                  <label className={`style-option ${config.voiceFeedbackStyle === "fun" ? "style-option--active" : ""}`}>
                    <input type="radio" name="style" value="fun" checked={config.voiceFeedbackStyle === "fun"}
                      onChange={() => setConfig(c => ({ ...c, voiceFeedbackStyle: "fun" }))} />
                    <span className="style-label">С характером</span>
                    <span className="style-example">«Слушаюсь, шеф!»</span>
                  </label>
                </div>

                <label className="field-label" style={{ marginTop: 12 }}>Голосовой движок</label>
                <div className="style-selector">
                  {(["system", "piper", "custom"] as const).map(eng => (
                    <label key={eng} className={`style-option ${config.voiceEngine === eng ? "style-option--active" : ""}`}>
                      <input type="radio" name="engine" value={eng} checked={config.voiceEngine === eng}
                        onChange={() => setConfig(c => ({ ...c, voiceEngine: eng }))} />
                      <span className="style-label">
                        {eng === "system" ? "Системный" : eng === "piper" ? "Piper TTS" : "Свой"}
                      </span>
                      <span className="style-example">
                        {eng === "system" ? "SAPI / espeak-ng" : eng === "piper" ? "офлайн, быстрый" : "своя команда"}
                      </span>
                    </label>
                  ))}
                </div>

                {/* ── Piper section ── */}
                {config.voiceEngine === "piper" && (
                  <div className="piper-section">
                    {/* Binary status */}
                    <div className="piper-binary-row">
                      <span className={`piper-binary-status ${piperStatus.binaryInstalled ? "piper-binary-status--ok" : ""}`}>
                        {piperStatus.binaryInstalled ? "● Piper установлен" : "● Piper не установлен"}
                      </span>
                      {!piperStatus.binaryInstalled && (
                        piperDownloading === "binary" ? (
                          <div className="piper-binary-dl">
                            <div className="piper-progress-wrap">
                              <div className="piper-progress-bar" style={{ width: `${piperProgress["binary"] ?? 0}%` }} />
                              <span className="piper-progress-label">{piperProgress["binary"] ?? 0}%</span>
                            </div>
                            <button className="btn-secondary btn-small" onClick={handleCancelPiper}>Отмена</button>
                          </div>
                        ) : (
                          <button className="btn-primary btn-small" onClick={handleDownloadBinary}>
                            Скачать (~20 MB)
                          </button>
                        )
                      )}
                    </div>

                    {/* Voice catalog */}
                    {piperStatus.binaryInstalled && (
                      <>
                        <label className="field-label" style={{ marginTop: 10 }}>Голос</label>
                        <div className="piper-voices">
                          {piperStatus.voices.map(v => {
                            const isSelected = config.piperVoice === v.id;
                            const isDl = piperDownloading === v.id;
                            const pct = piperProgress[v.id] ?? 0;
                            return (
                              <div key={v.id}
                                className={`piper-voice-card ${isSelected ? "piper-voice-card--selected" : ""}`}
                                onClick={() => v.installed && setConfig(c => ({ ...c, piperVoice: v.id }))}>
                                <div className="piper-voice-top">
                                  <div className="piper-voice-info">
                                    <span className="piper-voice-name">{v.displayName}</span>
                                    <span className="piper-voice-gender">{v.gender === "female" ? "♀" : "♂"}</span>
                                    <span className="piper-voice-size">{v.sizeMb} MB</span>
                                  </div>
                                  <div className="piper-voice-action">
                                    {v.installed ? (
                                      isSelected
                                        ? <span className="piper-voice-active">✓ Выбран</span>
                                        : <span className="piper-voice-installed">Установлен</span>
                                    ) : isDl ? (
                                      <button className="btn-danger btn-small" onClick={e => { e.stopPropagation(); handleCancelPiper(); }}>
                                        Отмена
                                      </button>
                                    ) : (
                                      <button className="btn-primary btn-small" onClick={e => { e.stopPropagation(); handleDownloadVoice(v.id); }}>
                                        Скачать
                                      </button>
                                    )}
                                  </div>
                                </div>
                                {isDl && (
                                  <div className="piper-progress-wrap" style={{ marginTop: 6 }}>
                                    <div className="piper-progress-bar" style={{ width: `${pct}%` }} />
                                    <span className="piper-progress-label">{pct}%</span>
                                  </div>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      </>
                    )}
                  </div>
                )}

                {config.voiceEngine === "custom" && (
                  <>
                    <label className="field-label" style={{ marginTop: 8 }}>
                      Команда <span className="hint">(<em>{"{text}"}</em> заменяется на текст)</span>
                    </label>
                    <input className="field-input monospace" placeholder='say "{text}"'
                      value={config.voiceCustomCmd}
                      onChange={e => setConfig(c => ({ ...c, voiceCustomCmd: e.target.value }))} />
                    <span className="field-hint" style={{ marginTop: 2 }}>
                      Примеры: <em>espeak-ng -v ru "{"{text}"}"</em> · <em>festival --tts &lt;&lt;&lt; "{"{text}"}"</em>
                    </span>
                  </>
                )}

                <label className="field-label" style={{ marginTop: 12 }}>Тест TTS</label>
                <div className="tts-test-row">
                  <input className="field-input" value={ttsTestText}
                    onChange={e => setTtsTestText(e.target.value)} placeholder="Текст для теста..." />
                  <button className="btn-secondary btn-small" onClick={handleTestTts}>▶ Произнести</button>
                </div>
              </>
            )}
          </div>

          {feedback && <div className="feedback-toast">{feedback}</div>}
        </section>
      )}

      {/* ── Footer ── */}
      <div className="panel-footer">
        {portWarning && <span className="port-warning">Порт изменён — перезапустите плагин</span>}
        <button className="btn-primary btn-save" onClick={handleSave}>
          {saved ? "✓ Сохранено" : "Сохранить"}
        </button>
      </div>

      {editTarget !== null && (
        <EditModal cmd={editTarget === "new" ? null : editTarget} categories={config.categories}
          onSave={handleEditSave} onClose={() => setEditTarget(null)} />
      )}
    </div>
  );
}
