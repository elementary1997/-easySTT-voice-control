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
}

interface OllamaModel {
  id: string;
  displayName: string;
  sizeMb: number;
  description: string;
  recommended: boolean;
  installed: boolean;
}

const DEFAULT_CONFIG: PluginConfig = {
  enabled: true, autostart: true,
  agentName: "Вилли", port: 8790,
  commands: [], categories: [],
  ollamaEnabled: false,
  ollamaUrl: "http://127.0.0.1:11434",
  ollamaModel: "llama3.2:1b",
  voiceFeedbackEnabled: false,
  voiceFeedbackStyle: "neutral",
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
function fmtMb(mb: number) { return mb >= 1000 ? `${(mb / 1000).toFixed(1)} GB` : `${mb} MB`; }

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
  const [ollamaOnline, setOllamaOnline] = useState<boolean | null>(null);
  const [modelCatalog, setModelCatalog] = useState<OllamaModel[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [pullProgress, setPullProgress] = useState<Record<string, number | null>>({});
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [ttsTestText, setTtsTestText] = useState("Привет! Я готов к работе.");

  // ── Init ──────────────────────────────────────────────────────────────────

  useEffect(() => {
    invoke<PluginConfig>("get_config").then(cfg => { setConfig(cfg); setOriginalPort(cfg.port); });
    invoke<string>("get_current_platform").then(p => setPlatform(p === "windows" ? "windows" : "linux"));
  }, []);

  // Ollama pull events
  useEffect(() => {
    const u1 = listen<{ model: string; percent: number | null; status: string }>(
      "ollama-pull-progress", ({ payload }) => {
        setPullProgress(p => ({ ...p, [payload.model]: payload.percent ?? null }));
      }
    );
    const u2 = listen<{ model: string }>("ollama-pull-done", ({ payload }) => {
      setPullingModel(null);
      setPullProgress(p => { const n = { ...p }; delete n[payload.model]; return n; });
      loadCatalog();
      showFeedback("Модель загружена!");
    });
    const u3 = listen<{ model: string; error: string }>("ollama-pull-error", ({ payload }) => {
      setPullingModel(null);
      setPullProgress(p => { const n = { ...p }; delete n[payload.model]; return n; });
      showFeedback(`Ошибка загрузки: ${payload.error}`);
    });
    return () => { u1.then(f => f()); u2.then(f => f()); u3.then(f => f()); };
  }, []);

  // ── Helpers ───────────────────────────────────────────────────────────────

  const showFeedback = (msg: string, ms = 3500) => {
    setFeedback(msg);
    setTimeout(() => setFeedback(""), ms);
  };

  const loadCatalog = useCallback(async () => {
    setCatalogLoading(true);
    try {
      const online = await invoke<boolean>("check_ollama", { url: config.ollamaUrl });
      setOllamaOnline(online);
      if (online) {
        const catalog = await invoke<OllamaModel[]>("get_ollama_catalog");
        setModelCatalog(catalog);
      }
    } finally {
      setCatalogLoading(false);
    }
  }, [config.ollamaUrl]);

  useEffect(() => {
    if (mainTab === "ai") loadCatalog();
  }, [mainTab, loadCatalog]);

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

  const handlePullModel = useCallback((modelId: string) => {
    setPullingModel(modelId);
    setPullProgress(p => ({ ...p, [modelId]: 0 }));
    invoke("pull_ollama_model", { modelId });
  }, []);

  const handleCancelPull = useCallback(() => {
    invoke("cancel_ollama_pull");
    setPullingModel(null);
  }, []);

  const handleTestTts = useCallback(() => {
    invoke("test_tts", { text: ttsTestText }).catch(e => showFeedback(`TTS ошибка: ${e}`));
  }, [ttsTestText]);

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
                <input ref={() => {}} className="cat-tab-input" placeholder="Название..." value={newCatInput} autoFocus
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

          {/* Ollama toggle */}
          <div className="ai-block">
            <div className="ai-block-header">
              <div>
                <h2 className="section-title" style={{ marginBottom: 2 }}>Ollama NLU</h2>
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
                  <label className="field-label">URL Ollama</label>
                  <input className="field-input monospace" value={config.ollamaUrl}
                    onChange={e => setConfig(c => ({ ...c, ollamaUrl: e.target.value }))} />
                </div>
                <button className="btn-secondary btn-small ai-check-btn" onClick={loadCatalog} disabled={catalogLoading}>
                  {catalogLoading ? "Проверяю..." : "Проверить"}
                </button>
              </div>

              {/* Ollama status */}
              {ollamaOnline !== null && (
                <div className={`ollama-status ${ollamaOnline ? "ollama-status--online" : "ollama-status--offline"}`}>
                  {ollamaOnline ? "● Ollama запущена" : "● Ollama недоступна — запустите: ollama serve"}
                </div>
              )}

              {/* Model catalog */}
              {ollamaOnline && (
                <>
                  <label className="field-label" style={{ marginTop: 10 }}>Модель</label>
                  <div className="model-catalog">
                    {modelCatalog.map(m => {
                      const isSelected = config.ollamaModel === m.id;
                      const isPulling = pullingModel === m.id;
                      const progress = pullProgress[m.id];
                      return (
                        <div key={m.id} className={`model-card ${isSelected ? "model-card--selected" : ""}`}
                          onClick={() => !isPulling && setConfig(c => ({ ...c, ollamaModel: m.id }))}>
                          <div className="model-card-top">
                            <div className="model-card-info">
                              <span className="model-card-name">{m.displayName}</span>
                              {m.recommended && <span className="model-badge">Рекомендуется</span>}
                              <span className="model-card-size">{fmtMb(m.sizeMb)}</span>
                            </div>
                            <div className="model-card-action">
                              {m.installed ? (
                                isSelected
                                  ? <span className="model-selected-mark">✓ Выбрана</span>
                                  : <span className="model-installed-mark">Установлена</span>
                              ) : isPulling ? (
                                <button className="btn-danger btn-small" onClick={e => { e.stopPropagation(); handleCancelPull(); }}>
                                  Отмена
                                </button>
                              ) : (
                                <button className="btn-primary btn-small" onClick={e => { e.stopPropagation(); handlePullModel(m.id); }}>
                                  Скачать
                                </button>
                              )}
                            </div>
                          </div>
                          <p className="model-card-desc">{m.description}</p>
                          {isPulling && (
                            <div className="model-progress-wrap">
                              <div className="model-progress-bar"
                                style={{ width: `${progress ?? 0}%` }} />
                              <span className="model-progress-label">
                                {progress != null ? `${Math.round(progress)}%` : "Подключаюсь..."}
                              </span>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
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

                <label className="field-label" style={{ marginTop: 10 }}>Тест TTS</label>
                <div className="tts-test-row">
                  <input className="field-input" value={ttsTestText}
                    onChange={e => setTtsTestText(e.target.value)} placeholder="Текст для теста..." />
                  <button className="btn-secondary btn-small" onClick={handleTestTts}>▶ Произнести</button>
                </div>
                <span className="field-hint" style={{ marginTop: 4 }}>
                  Windows: SAPI (системный голос) · Linux: espeak-ng
                </span>
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
