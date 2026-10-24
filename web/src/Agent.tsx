import { useEffect, useRef, useState } from "react";
import {
  ArrowUp,
  Brain,
  ChevronDown,
  Cpu,
  History,
  LoaderCircle,
  Save,
  ShieldAlert,
  SlidersHorizontal,
  Sparkles,
  SquarePen,
  Trash2,
  Wrench,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { api, errorMessage } from "./api";
import { Field } from "./components";
import { loadPrefs } from "./prefs";
import type { Prefs } from "./prefs";

type AgentConfig = {
  endpoint: string;
  api_key: string;
  model: string;
  group_id: string;
  system_prompt: string;
  allow_root: boolean;
  max_steps: number;
  reasoning: string;
};

type ChatMessage = { role: "user" | "assistant"; content: string; reasoning?: string; ts?: number };
type ConversationSummary = {
  id: string;
  title: string;
  model: string;
  updated_at: number;
  message_count: number;
};
type Conversation = {
  id: string;
  title: string;
  model: string;
  messages: ChatMessage[];
  updated_at: number;
};

function newConversationId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
type ToolEvent = { name: string; args: unknown; result?: string; done: boolean };
type ModelGroup = { provider: string; models: string[] };
type Notice = (text: string, error?: boolean) => void;

const emptyConfig: AgentConfig = {
  endpoint: "",
  api_key: "",
  model: "",
  group_id: "",
  system_prompt: "",
  allow_root: false,
  max_steps: 6,
  reasoning: "",
};

async function loadConfig(): Promise<AgentConfig> {
  const cfg = await api<AgentConfig>("/agent/config");
  return { ...emptyConfig, ...cfg };
}

/* ------------------------------------------------------------------ */
/* 模型接入 / 助手设置                                                 */
/* ------------------------------------------------------------------ */
export function AgentSettingsPage({
  notify,
  onDone,
  onOpenAccounts,
}: {
  notify: Notice;
  onDone: () => void;
  onOpenAccounts: () => void;
}) {
  const [config, setConfig] = useState<AgentConfig>(emptyConfig);
  const [models, setModels] = useState<ModelGroup[]>([]);
  const [groups, setGroups] = useState<{ id: string; name: string }[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        const [cfg, list, glist] = await Promise.all([
          loadConfig(),
          api<ModelGroup[]>("/agent/models"),
          api<{ id: string; name: string }[]>("/api/groups"),
        ]);
        setModels(list);
        setGroups(Array.isArray(glist) ? glist : []);
        setConfig({
          ...cfg,
          model: cfg.model || list[0]?.models[0] || "",
          endpoint: "",
          api_key: "",
        });
      } catch (failure) {
        setError(errorMessage(failure));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  async function save() {
    setSaving(true);
    setError("");
    try {
      await api("/agent/config", "PUT", {
        ...config,
        endpoint: "",
        api_key: "",
      });
      notify("助手设置已保存");
      onDone();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="rk-settings-page">
      <div className="page-heading">
        <div>
          <div className="eyebrow">ASSISTANT SETTINGS</div>
          <h1>模型接入</h1>
          <p>直接使用「账号管理」里的模型，无需再填端点与 Key</p>
        </div>
        <button
          type="button"
          className="rk-primary"
          onClick={() => void save()}
          disabled={saving || loading || !config.model}
        >
          {saving ? <LoaderCircle className="spin" size={15} /> : <Save size={15} />}
          保存
        </button>
      </div>

      {error ? <div className="rk-error">{error}</div> : null}

      <section className="rk-settings">
        {loading ? (
          <div className="rk-loading">
            <LoaderCircle className="spin" size={16} />
            正在读取模型…
          </div>
        ) : models.length === 0 ? (
          <div className="rk-no-models">
            <strong>账号管理里还没有可用的 chat 模型</strong>
            <p>先在「账号管理」添加账号，勾选 chat 协议并选择模型，这里就会出现。</p>
            <button type="button" className="rk-primary" onClick={onOpenAccounts}>
              去账号管理
            </button>
          </div>
        ) : (
          <div className="rk-form">
            <Field label="模型" hint="来自账号管理中启用的账号（需支持 chat 协议）">
              <select
                value={config.model}
                onChange={(event) => setConfig({ ...config, model: event.target.value })}
              >
                {models.map((group) => (
                  <optgroup key={group.provider} label={group.provider}>
                    {group.models.map((model) => (
                      <option key={model} value={model}>
                        {model}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </select>
            </Field>
            <Field label="破甲分组" hint="聊天时选取该分组的破甲策略；留空跟随账号所属分组">
              <select
                value={config.group_id || ""}
                onChange={(event) =>
                  setConfig({ ...config, group_id: event.target.value })
                }
              >
                <option value="">跟随账号分组</option>
                {groups.map((group) => (
                  <option key={group.id} value={group.id}>
                    {group.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="最大步数" hint="单轮最多调用工具的次数">
              <input
                type="number"
                min={1}
                max={20}
                value={config.max_steps}
                onChange={(event) =>
                  setConfig({ ...config, max_steps: Number(event.target.value) || 1 })
                }
              />
            </Field>
            <div className="rk-form-wide">
              <Field label="系统提示词" hint="留空则使用默认提示词">
                <textarea
                  rows={4}
                  value={config.system_prompt}
                  onChange={(event) =>
                    setConfig({ ...config, system_prompt: event.target.value })
                  }
                />
              </Field>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* 智能助手（聊天页面）                                                */
/* ------------------------------------------------------------------ */
function ThinkingBlock({
  text,
  streaming,
  defaultOpen,
}: {
  text: string;
  streaming: boolean;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="rk-thinking">
      <button
        type="button"
        className="rk-thinking-head"
        onClick={() => setOpen((current) => !current)}
      >
        <Brain size={14} />
        <span>思考过程</span>
        {streaming ? <LoaderCircle className="spin" size={12} /> : null}
        <ChevronDown
          size={14}
          className={`rk-thinking-chevron ${open ? "open" : ""}`}
        />
      </button>
      {open ? <div className="rk-thinking-text">{text}</div> : null}
    </div>
  );
}

function timeLabel(ts?: number) {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function AgentChatPage({
  onOpenSettings,
}: {
  onOpenSettings: () => void;
}) {
  const [config, setConfig] = useState<AgentConfig>(emptyConfig);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [useRoot, setUseRoot] = useState(() => loadPrefs().defaultRoot);
  const [prefs, setPrefs] = useState<Prefs>(() => loadPrefs());
  const [tools, setTools] = useState<ToolEvent[]>([]);
  const [models, setModels] = useState<ModelGroup[]>([]);
  const [picker, setPicker] = useState<"model" | "thinking" | null>(null);
  const [modelQuery, setModelQuery] = useState("");
  const [collapsedProviders, setCollapsedProviders] = useState<Record<string, boolean>>({});
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const sync = () => setPrefs(loadPrefs());
    window.addEventListener("lxsh-prefs", sync);
    return () => window.removeEventListener("lxsh-prefs", sync);
  }, []);

  useEffect(() => {
    void (async () => {
      try {
        const [cfg, list, summary] = await Promise.all([
          loadConfig(),
          api<ModelGroup[]>("/agent/models"),
          api<ConversationSummary[]>("/agent/conversations"),
        ]);
        setConfig(cfg);
        setModels(list);
        setConversations(summary);
        if (summary.length > 0) {
          const current = await api<Conversation>(
            `/agent/conversations/${summary[0].id}`,
          );
          setConversationId(current.id);
          setHistory(current.messages ?? []);
        }
      } catch (failure) {
        setError(errorMessage(failure));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  async function choose(patch: Partial<AgentConfig>) {
    const next = { ...config, ...patch };
    setConfig(next);
    setPicker(null);
    try {
      await api("/agent/config", "PUT", {
        ...next,
        endpoint: "",
        api_key: "",
      });
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  function thinkingLabel(value: string) {
    if (value === "low") return "思考:低";
    if (value === "medium") return "思考:中";
    if (value === "high") return "思考:高";
    return "思考:自动";
  }

  useEffect(() => {
    if (history.length || tools.length) {
      endRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [history, tools]);

  const configured = Boolean(config.model);
  const filteredModelGroups = models
    .map((group) => ({
      ...group,
      models: group.models.filter((model) =>
        model.toLowerCase().includes(modelQuery.trim().toLowerCase()),
      ),
    }))
    .filter((group) => group.models.length > 0);

  async function send() {
    const text = input.trim();
    if (!text || streaming || !configured) return;
    setInput("");
    setError("");
    setTools([]);
    const now = Math.floor(Date.now() / 1000);
    const nextHistory: ChatMessage[] = [
      ...history,
      { role: "user", content: text, ts: now },
    ];
    setHistory([...nextHistory, { role: "assistant", content: "", ts: now }]);
    setStreaming(true);
    let assistant = "";
    let reasoning = "";
    try {
      const response = await fetch("/api/agent/chat", {
        method: "POST",
        credentials: "same-origin",
        headers: {
          "Content-Type": "application/json",
          "X-Console-Path": window.location.pathname,
        },
        body: JSON.stringify({ messages: nextHistory, allow_root: useRoot }),
      });
      if (!response.ok || !response.body) {
        const detail = await response.text();
        throw new Error(detail || `HTTP ${response.status}`);
      }
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const blocks = buffer.split("\n\n");
        buffer = blocks.pop() ?? "";
        for (const block of blocks) {
          let event = "message";
          const dataLines: string[] = [];
          for (const line of block.split("\n")) {
            if (line.startsWith("event:")) event = line.slice(6).trim();
            else if (line.startsWith("data:")) dataLines.push(line.slice(5).trim());
          }
          if (!dataLines.length) continue;
          let data: Record<string, any>;
          try {
            data = JSON.parse(dataLines.join("\n"));
          } catch {
            continue;
          }
          if (event === "delta") {
            assistant += typeof data.text === "string" ? data.text : "";
            setHistory((current) => {
              const copy = [...current];
              const index = copy.length - 1;
              if (index >= 0) copy[index] = { role: "assistant", content: assistant };
              return copy;
            });
          } else if (event === "reasoning") {
            reasoning += typeof data.text === "string" ? data.text : "";
            setHistory((current) => {
              const copy = [...current];
              const index = copy.length - 1;
              if (index >= 0) {
                copy[index] = { ...copy[index], reasoning };
              }
              return copy;
            });
          } else if (event === "tool") {
            setTools((current) => [
              ...current,
              { name: String(data.name ?? ""), args: data.args, done: false },
            ]);
          } else if (event === "tool_result") {
            setTools((current) => {
              const copy = [...current];
              for (let i = copy.length - 1; i >= 0; i -= 1) {
                if (copy[i].name === data.name && !copy[i].done) {
                  copy[i] = { ...copy[i], result: String(data.result ?? ""), done: true };
                  break;
                }
              }
              return copy;
            });
          } else if (event === "error") {
            setError(String(data.message ?? "助手出错"));
          }
        }
      }
      if (!assistant) setHistory((current) => current.slice(0, -1));
    } catch (failure) {
      setError(errorMessage(failure));
      setHistory((current) => current.slice(0, -1));
    } finally {
      setStreaming(false);
      if (nextHistory.length > 0) {
        const finalMessages = assistant || reasoning
          ? [
              ...nextHistory,
              { role: "assistant" as const, content: assistant, reasoning, ts: Math.floor(Date.now() / 1000) },
            ]
          : nextHistory;
        void persistConversation(finalMessages);
      }
    }
  }

  async function refreshConversations() {
    try {
      setConversations(await api<ConversationSummary[]>("/agent/conversations"));
    } catch {
      /* ignore */
    }
  }

  async function persistConversation(messages: ChatMessage[]) {
    const id = conversationId ?? newConversationId();
    if (!conversationId) setConversationId(id);
    const firstUser = messages.find((message) => message.role === "user")?.content;
    const title = (firstUser || "新对话").slice(0, 40);
    try {
      await api(`/agent/conversations/${id}`, "PUT", {
        title,
        model: config.model,
        messages,
      });
      void refreshConversations();
    } catch {
      /* history persistence is best-effort */
    }
  }

  async function loadConversation(id: string) {
    try {
      const current = await api<Conversation>(`/agent/conversations/${id}`);
      setConversationId(current.id);
      setHistory(current.messages ?? []);
      setTools([]);
      setError("");
      setHistoryOpen(false);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  async function deleteConversation(id: string) {
    try {
      await api(`/agent/conversations/${id}`, "DELETE");
      setConversations((current) => current.filter((item) => item.id !== id));
      if (conversationId === id) resetChat();
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  function resetChat() {
    setConversationId(null);
    setHistory([]);
    setTools([]);
    setError("");
    setHistoryOpen(false);
  }

  return (
    <div
      className={`rk-agent rk-font-${prefs.fontSize} rk-style-${prefs.messageStyle} ${
        prefs.compact ? "rk-compact" : ""
      } ${prefs.wrapCode ? "" : "rk-nowrap-code"}`}
    >
      <header className="rk-header">
        <div className="rk-header-left">
          <span className="rk-spark">
            <Sparkles size={22} />
          </span>
          <div className="rk-heading">
            <strong>{history.length ? "对话" : "你好，有什么可以帮您"}</strong>
            <span>{config.model || "未选择模型"}</span>
          </div>
        </div>
        <div className="rk-header-actions">
          <button
            type="button"
            className="rk-icon"
            title="对话历史"
            onClick={() => setHistoryOpen((open) => !open)}
          >
            <History size={19} />
          </button>
          <button type="button" className="rk-icon" title="模型接入" onClick={onOpenSettings}>
            <SlidersHorizontal size={19} />
          </button>
          <button
            type="button"
            className="rk-icon"
            title="新建对话"
            onClick={resetChat}
            disabled={streaming}
          >
            <SquarePen size={19} />
          </button>
        </div>
      </header>

      {historyOpen ? (
        <div className="rk-history">
          <div className="rk-history-head">
            <strong>对话历史</strong>
            <button type="button" className="rk-history-new" onClick={resetChat}>
              <SquarePen size={14} />
              新建
            </button>
          </div>
          {conversations.length === 0 ? (
            <div className="rk-history-empty">还没有历史对话</div>
          ) : (
            conversations.map((item) => (
              <div
                key={item.id}
                className={`rk-history-item ${item.id === conversationId ? "active" : ""}`}
              >
                <button
                  type="button"
                  className="rk-history-open"
                  onClick={() => void loadConversation(item.id)}
                >
                  <span>{item.title}</span>
                  <small>
                    {new Date(item.updated_at * 1000).toLocaleString()}
                  </small>
                </button>
                <button
                  type="button"
                  className="rk-history-del"
                  title="删除对话"
                  onClick={() => void deleteConversation(item.id)}
                >
                  <Trash2 size={15} />
                </button>
              </div>
            ))
          )}
        </div>
      ) : null}

      {error ? <div className="rk-error">{error}</div> : null}

      <div className="rk-chat">
        {!configured ? (
          <div className="rk-empty">
            <span className="rk-empty-spark">
              <Sparkles size={30} />
            </span>
            <strong>还没有选择模型</strong>
            <p>模型来自「账号管理」，先选一个再开始对话</p>
            <button type="button" className="rk-primary" onClick={onOpenSettings}>
              <SlidersHorizontal size={15} />
              去选择模型
            </button>
          </div>
        ) : history.length === 0 && tools.length === 0 ? (
          <div className="rk-empty">
            <span className="rk-empty-spark">
              <Sparkles size={30} />
            </span>
            <strong>你好，有什么可以帮您</strong>
            <p>可以试试「查看当前目录」或「跑一下 id」</p>
          </div>
        ) : null}

        {history.map((message, index) =>
          message.role === "user" ? (
            <div key={index} className="rk-user-block">
              <div className="rk-user">{message.content}</div>
              {prefs.showTime && message.ts ? (
                <span className="rk-time">{timeLabel(message.ts)}</span>
              ) : null}
            </div>
          ) : (
            <div key={index} className="rk-assistant">
              {prefs.showModelLabel ? (
                <div className="rk-assistant-label">
                  <Sparkles size={14} />
                  <span>{(config.model || "ASSISTANT").toUpperCase()}</span>
                </div>
              ) : null}
              {message.reasoning ? (
                <ThinkingBlock
                  text={message.reasoning}
                  streaming={streaming && index === history.length - 1}
                  defaultOpen={prefs.thinkingOpen}
                />
              ) : null}
              <div className="rk-assistant-text">
                {message.content ? (
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                    {message.content}
                  </ReactMarkdown>
                ) : streaming ? (
                  "…"
                ) : (
                  ""
                )}
              </div>
              {prefs.showTime && message.ts ? (
                <span className="rk-time">{timeLabel(message.ts)}</span>
              ) : null}
            </div>
          ),
        )}

        {prefs.showTools
          ? tools.map((tool, index) => (
              <details key={`tool-${index}`} className="rk-tool">
                <summary>
                  <Wrench size={14} />
                  <span>工具 · {tool.name}</span>
                  {tool.done ? null : <LoaderCircle className="spin" size={13} />}
                </summary>
                <pre>{JSON.stringify(tool.args, null, 2)}</pre>
                {tool.result !== undefined ? (
                  <pre className="rk-tool-out">{tool.result}</pre>
                ) : null}
              </details>
            ))
          : null}
        <div ref={endRef} />
      </div>

      <div className="rk-composer">
        {picker === "model" ? (
          <div className="rk-picker rk-picker-models">
            <input
              className="rk-picker-search"
              placeholder="搜索模型"
              value={modelQuery}
              onChange={(event) => setModelQuery(event.target.value)}
            />
            {models.length === 0 ? (
              <div className="rk-picker-empty">账号管理里没有可用的 chat 模型</div>
            ) : filteredModelGroups.length === 0 ? (
              <div className="rk-picker-empty">没有匹配的模型</div>
            ) : (
              filteredModelGroups.map((group) => {
                const collapsed =
                  modelQuery.trim() === "" && Boolean(collapsedProviders[group.provider]);
                return (
                  <div key={group.provider} className="rk-picker-group">
                    <button
                      type="button"
                      className="rk-picker-group-title"
                      onClick={() =>
                        setCollapsedProviders((current) => ({
                          ...current,
                          [group.provider]: !current[group.provider],
                        }))
                      }
                    >
                      <ChevronDown size={13} className={collapsed ? "" : "open"} />
                      <span>{group.provider}</span>
                      <small>{group.models.length}</small>
                    </button>
                    {collapsed
                      ? null
                      : group.models.map((model) => (
                          <button
                            type="button"
                            key={model}
                            className={`rk-picker-item ${model === config.model ? "active" : ""}`}
                            onClick={() => void choose({ model })}
                          >
                            {model}
                          </button>
                        ))}
                  </div>
                );
              })
            )}
          </div>
        ) : null}
        {picker === "thinking" ? (
          <div className="rk-picker">
            {[
              { value: "", label: "自动（不指定）" },
              { value: "low", label: "低" },
              { value: "medium", label: "中" },
              { value: "high", label: "高" },
            ].map((option) => (
              <button
                type="button"
                key={option.value || "auto"}
                className={`rk-picker-item ${config.reasoning === option.value ? "active" : ""}`}
                onClick={() => void choose({ reasoning: option.value })}
              >
                {option.label}
              </button>
            ))}
          </div>
        ) : null}
        <div className="rk-tools-row">
          <label className="rk-root-chip">
            <input
              type="checkbox"
              checked={useRoot}
              onChange={(event) => setUseRoot(event.target.checked)}
            />
            <ShieldAlert size={12} />
            root
          </label>
          <button
            type="button"
            className="rk-chip"
            onClick={() => {
              setPicker((current) => (current === "model" ? null : "model"));
              setModelQuery("");
            }}
          >
            <Cpu size={12} />
            <span>{config.model || "选择模型"}</span>
          </button>
          <button
            type="button"
            className="rk-chip"
            onClick={() => setPicker((current) => (current === "thinking" ? null : "thinking"))}
          >
            <Brain size={12} />
            <span>{thinkingLabel(config.reasoning)}</span>
          </button>
        </div>
        <div className={`rk-input ${!configured ? "is-disabled" : ""}`}>
          <div className="rk-input-main">
            <textarea
              rows={1}
              value={input}
              placeholder={loading ? "正在读取配置…" : "输入消息与 AI 聊天"}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.nativeEvent.isComposing) return;
                const shouldSend = prefs.enterToSend
                  ? event.key === "Enter" && !event.shiftKey
                  : event.key === "Enter" && (event.ctrlKey || event.metaKey);
                if (shouldSend) {
                  event.preventDefault();
                  void send();
                }
              }}
              disabled={streaming || loading || !configured}
            />
            <button
              type="button"
              className="rk-send"
              onClick={() => void send()}
              disabled={!input.trim() || loading || streaming || !configured}
              title="发送"
            >
              {streaming ? <LoaderCircle className="spin" size={18} /> : <ArrowUp size={18} />}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
