import { useEffect, useRef, useState } from "react";
import {
  ArrowUp,
  Brain,
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

type AgentConfig = {
  endpoint: string;
  api_key: string;
  model: string;
  system_prompt: string;
  allow_root: boolean;
  max_steps: number;
  reasoning: string;
};

type ChatMessage = { role: "user" | "assistant"; content: string };
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
type Notice = (text: string, error?: boolean) => void;

const emptyConfig: AgentConfig = {
  endpoint: "",
  api_key: "",
  model: "",
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
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        const [cfg, list] = await Promise.all([
          loadConfig(),
          api<string[]>("/agent/models"),
        ]);
        setModels(list);
        setConfig({
          ...cfg,
          model: cfg.model || list[0] || "",
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
                {models.map((model) => (
                  <option key={model} value={model}>
                    {model}
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
  const [useRoot, setUseRoot] = useState(true);
  const [tools, setTools] = useState<ToolEvent[]>([]);
  const [models, setModels] = useState<string[]>([]);
  const [picker, setPicker] = useState<"model" | "thinking" | null>(null);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [cfg, list, summary] = await Promise.all([
          loadConfig(),
          api<string[]>("/agent/models"),
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

  async function send() {
    const text = input.trim();
    if (!text || streaming || !configured) return;
    setInput("");
    setError("");
    setTools([]);
    const nextHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    setHistory([...nextHistory, { role: "assistant", content: "" }]);
    setStreaming(true);
    let assistant = "";
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
        const finalMessages = assistant
          ? [...nextHistory, { role: "assistant" as const, content: assistant }]
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
    <div className="rk-agent">
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
            <div key={index} className="rk-user">
              {message.content}
            </div>
          ) : (
            <div key={index} className="rk-assistant">
              <div className="rk-assistant-label">
                <Sparkles size={14} />
                <span>{(config.model || "ASSISTANT").toUpperCase()}</span>
              </div>
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
            </div>
          ),
        )}

        {tools.map((tool, index) => (
          <details key={`tool-${index}`} className="rk-tool">
            <summary>
              <Wrench size={14} />
              <span>工具 · {tool.name}</span>
              {tool.done ? null : <LoaderCircle className="spin" size={13} />}
            </summary>
            <pre>{JSON.stringify(tool.args, null, 2)}</pre>
            {tool.result !== undefined ? <pre className="rk-tool-out">{tool.result}</pre> : null}
          </details>
        ))}
        <div ref={endRef} />
      </div>

      <div className="rk-composer">
        <div className="rk-controls">
          <button
            type="button"
            className="rk-chip"
            onClick={() => setPicker((current) => (current === "model" ? null : "model"))}
          >
            <Cpu size={13} />
            <span>{config.model || "选择模型"}</span>
          </button>
          <button
            type="button"
            className="rk-chip"
            onClick={() =>
              setPicker((current) => (current === "thinking" ? null : "thinking"))
            }
          >
            <Brain size={13} />
            <span>{thinkingLabel(config.reasoning)}</span>
          </button>
        </div>
        {picker === "model" ? (
          <div className="rk-picker">
            {models.length === 0 ? (
              <div className="rk-picker-empty">账号管理里没有可用的 chat 模型</div>
            ) : (
              models.map((model) => (
                <button
                  type="button"
                  key={model}
                  className={`rk-picker-item ${model === config.model ? "active" : ""}`}
                  onClick={() => void choose({ model })}
                >
                  {model}
                </button>
              ))
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
        <label className="rk-root-chip">
          <input
            type="checkbox"
            checked={useRoot}
            onChange={(event) => setUseRoot(event.target.checked)}
          />
          <ShieldAlert size={13} />
          root
        </label>
        <div className={`rk-input ${!configured ? "is-disabled" : ""}`}>
          <textarea
            rows={1}
            value={input}
            placeholder={loading ? "正在读取配置…" : "输入消息与 AI 聊天"}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
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
  );
}
