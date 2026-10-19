import { useRef, useState } from "react";
import { Copy, Info, Trash2 } from "lucide-react";
import { api, copy, errorMessage } from "./api";
import { Button, Field, Toggle } from "./components";
import { loadPrefs, savePrefs } from "./prefs";
import type { Prefs } from "./prefs";
import { version } from "../package.json";

function ToggleRow({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="prefs-row">
      <span>
        <strong>{label}</strong>
        {hint ? <small>{hint}</small> : null}
      </span>
      <Toggle label="" checked={checked} onChange={onChange} />
    </div>
  );
}

async function compressImage(file: File, maxSize = 1600): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const image = new window.Image();
      image.onload = () => {
        const scale = Math.min(1, maxSize / Math.max(image.width, image.height));
        const canvas = document.createElement("canvas");
        canvas.width = Math.max(1, Math.round(image.width * scale));
        canvas.height = Math.max(1, Math.round(image.height * scale));
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          reject(new Error("无法处理图片"));
          return;
        }
        ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
        resolve(canvas.toDataURL("image/jpeg", 0.75));
      };
      image.onerror = () => reject(new Error("无法读取图片"));
      image.src = String(reader.result ?? "");
    };
    reader.onerror = () => reject(new Error("读取图片失败"));
    reader.readAsDataURL(file);
  });
}

export function PersonalizationPage({
  notify,
}: {
  notify: (text: string, error?: boolean) => void;
}) {
  const [prefs, setPrefs] = useState<Prefs>(() => loadPrefs());
  const [busy, setBusy] = useState("");
  const bgFileInput = useRef<HTMLInputElement>(null);

  async function pickBackground(file: File | undefined) {
    if (!file) return;
    setBusy("bg");
    try {
      if (!file.type.startsWith("image/")) {
        throw new Error("请选择图片文件");
      }
      const data = await compressImage(file);
      update({ bgImage: data, uiBackground: "image" });
      notify("背景图片已应用");
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setBusy("");
    }
  }

  function update(patch: Partial<Prefs>) {
    const next = { ...prefs, ...patch };
    setPrefs(next);
    savePrefs(next);
  }

  async function exportData() {
    setBusy("export");
    try {
      const list = await api<{ id: string }[]>("/agent/conversations");
      const data: unknown[] = [];
      for (const item of list) {
        data.push(await api(`/agent/conversations/${item.id}`));
      }
      await copy(JSON.stringify(data, null, 2));
      notify(`已复制 ${data.length} 个对话的 JSON 到剪贴板`);
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setBusy("");
    }
  }

  async function clearHistory() {
    if (!window.confirm("确定清空所有对话历史？此操作不可恢复。")) return;
    setBusy("clear");
    try {
      await api("/agent/conversations", "DELETE");
      notify("对话历史已清空");
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="rk-settings-page">
      <div className="page-heading">
        <div>
          <div className="eyebrow">PERSONALIZATION</div>
          <h1>个性化</h1>
          <p>聊天外观、内容展示与交互习惯</p>
        </div>
      </div>

      <section className="rk-settings">
        <div className="prefs-grid">
          <Field label="字体大小">
            <select
              value={prefs.fontSize}
              onChange={(event) =>
                update({ fontSize: event.target.value as Prefs["fontSize"] })
              }
            >
              <option value="small">小</option>
              <option value="normal">标准</option>
              <option value="large">大</option>
            </select>
          </Field>
          <Field label="界面字体">
            <select
              value={prefs.fontFamily}
              onChange={(event) =>
                update({ fontFamily: event.target.value as Prefs["fontFamily"] })
              }
            >
              <option value="default">默认（Inter）</option>
              <option value="system">系统字体</option>
              <option value="serif">衬线</option>
              <option value="mono">等宽</option>
            </select>
          </Field>
          <Field label="界面背景">
            <select
              value={prefs.uiBackground}
              onChange={(event) =>
                update({ uiBackground: event.target.value as Prefs["uiBackground"] })
              }
            >
              <option value="aurora">极光（默认）</option>
              <option value="plain">纯净浅色</option>
              <option value="sunset">晚霞</option>
              <option value="ocean">海洋</option>
              <option value="image">自定义图片</option>
            </select>
          </Field>
          <Field label="界面主题">
            <select
              value={prefs.uiTheme}
              onChange={(event) =>
                update({ uiTheme: event.target.value as Prefs["uiTheme"] })
              }
            >
              <option value="light">浅色</option>
              <option value="dark">深色</option>
              <option value="auto">跟随系统</option>
            </select>
          </Field>
          <Field label="背景图片">
            <div className="prefs-file-actions">
              <Button
                busy={busy === "bg"}
                type="button"
                onClick={() => bgFileInput.current?.click()}
              >
                {prefs.bgImage ? "更换图片" : "上传图片"}
              </Button>
              {prefs.bgImage ? (
                <Button
                  type="button"
                  onClick={() => update({ bgImage: "", uiBackground: "aurora" })}
                >
                  清除
                </Button>
              ) : null}
              <input
                ref={bgFileInput}
                type="file"
                accept="image/*"
                style={{ display: "none" }}
                onChange={(event) => {
                  void pickBackground(event.target.files?.[0]);
                  event.target.value = "";
                }}
              />
            </div>
          </Field>
          <Field label="消息样式">
            <select
              value={prefs.messageStyle}
              onChange={(event) =>
                update({ messageStyle: event.target.value as Prefs["messageStyle"] })
              }
            >
              <option value="plain">简洁</option>
              <option value="bubble">气泡</option>
            </select>
          </Field>
        </div>

        <div className="prefs-rows">
          <ToggleRow
            label="思考过程默认展开"
            hint="有思考内容时直接显示，而不是折叠"
            checked={prefs.thinkingOpen}
            onChange={(thinkingOpen) => update({ thinkingOpen })}
          />
          <ToggleRow
            label="显示工具调用"
            hint="展示 Agent 调用了哪些工具"
            checked={prefs.showTools}
            onChange={(showTools) => update({ showTools })}
          />
          <ToggleRow
            label="显示消息时间"
            hint="在消息下方显示发送时间"
            checked={prefs.showTime}
            onChange={(showTime) => update({ showTime })}
          />
          <ToggleRow
            label="显示模型名称"
            hint="在助手消息上方显示模型标识"
            checked={prefs.showModelLabel}
            onChange={(showModelLabel) => update({ showModelLabel })}
          />
          <ToggleRow
            label="代码块自动换行"
            hint="关闭后长代码改为横向滚动"
            checked={prefs.wrapCode}
            onChange={(wrapCode) => update({ wrapCode })}
          />
          <ToggleRow
            label="紧凑模式"
            hint="减小消息间距，一屏显示更多"
            checked={prefs.compact}
            onChange={(compact) => update({ compact })}
          />
        </div>

        <div className="prefs-rows">
          <ToggleRow
            label="回车发送"
            hint="关闭后回车换行，Ctrl/⌘+回车发送"
            checked={prefs.enterToSend}
            onChange={(enterToSend) => update({ enterToSend })}
          />
          <ToggleRow
            label="root 默认开启"
            hint="进入对话时默认勾选 root"
            checked={prefs.defaultRoot}
            onChange={(defaultRoot) => update({ defaultRoot })}
          />
        </div>

        <div className="prefs-actions">
          <Button type="button" busy={busy === "export"} onClick={() => void exportData()}>
            <Copy size={15} />
            复制对话 JSON
          </Button>
          <Button
            type="button"
            className="danger"
            busy={busy === "clear"}
            onClick={() => void clearHistory()}
          >
            <Trash2 size={15} />
            清空对话历史
          </Button>
        </div>

        <div className="prefs-about">
          <Info size={14} />
          ZakoAI {version} · 个性化设置保存在本机
        </div>
      </section>
    </div>
  );
}
