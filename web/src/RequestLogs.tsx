import { useEffect, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Download,
  FileClock,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { api, errorMessage, number, tokenCount } from "./api";
import type { RequestLog, RequestLogExport, RequestLogPage, State } from "./api";
import { Badge, Button, Confirm, Empty, ErrorBox, Select } from "./components";
import type { Notice } from "./components";

function duration(milliseconds: number | null | undefined) {
  if (milliseconds === null || milliseconds === undefined) return "—";
  if (milliseconds < 1000) return `${number(milliseconds)} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1000).toFixed(2)} s`;
  return `${(milliseconds / 60_000).toFixed(2)} min`;
}

function timestamp(value: number) {
  return new Date(value * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function Metric({
  label,
  value,
  tone = "neutral",
  title,
}: {
  label: string;
  value: string;
  tone?: "neutral" | "green" | "blue" | "amber" | "purple";
  title?: string;
}) {
  return (
    <span className={`log-metric log-metric-${tone}`} title={title}>
      <span>{label}</span>
      <strong>{value}</strong>
    </span>
  );
}

function LogRow({ row, state }: { row: RequestLog; state: State }) {
  const succeeded = row.status >= 200 && row.status < 300;
  const redirected = row.status >= 300 && row.status < 400;
  const statusLabel = succeeded ? "成功" : redirected ? "重定向" : "失败";
  const statusTone = succeeded ? "green" : redirected ? "amber" : "red";
  const account =
    row.account_name ||
    state.accounts.find((item) => item.id === row.account_id)?.name ||
    "未匹配账号";
  const key =
    row.key_name ||
    state.keys.find((item) => item.id === row.key_id)?.name ||
    "未匹配密钥";
  const cached = row.usage.cache_read_input_tokens ?? row.usage.cached_tokens;
  const created = row.usage.cache_creation_input_tokens ?? 0;
  const totalTokens = row.usage.input_tokens + row.usage.output_tokens;
  const cacheRate = row.usage.input_tokens
    ? `${Math.min(100, (cached / row.usage.input_tokens) * 100).toFixed(1)}%`
    : "—";
  const generationMs =
    row.first_token_ms === null || row.first_token_ms === undefined
      ? null
      : row.latency_ms - row.first_token_ms;
  const tps =
    generationMs !== null && generationMs > 0 && row.usage.output_tokens > 0
      ? ((row.usage.output_tokens * 1000) / generationMs).toFixed(2)
      : "—";
  return (
    <li>
      <details className={`request-log-row request-log-${statusTone}`}>
        <summary className="request-log-summary">
          <span className="request-log-overview">
            <time className="log-time mono" dateTime={new Date(row.timestamp * 1000).toISOString()}>
              {timestamp(row.timestamp)}
            </time>
            <strong className="log-model" title={row.model || "未指定模型"}>
              {row.model || "未指定模型"}
            </strong>
            <Badge>{row.protocol || "未知协议"}</Badge>
            <Badge variant={statusTone} dot>
              {row.status || "未响应"} {statusLabel}
            </Badge>
            {!!row.cyber && <Badge variant="amber">Cyber</Badge>}
            <span className="log-source" title={`账号：${account} · 密钥：${key}`}>
              {account} / {key}
            </span>
            <ChevronDown className="log-expand-icon" size={15} aria-hidden="true" />
            <span className="visually-hidden">展开或收起请求 {row.id} 的详情</span>
          </span>
          <span className="request-log-metrics">
            <span className="log-metric-group" aria-label="Token 用量与缓存">
              <Metric label="Token" value={tokenCount(totalTokens)} title={`合计 ${number(totalTokens)} Token`} />
              <Metric label="输入" value={tokenCount(row.usage.input_tokens)} tone="green" title={number(row.usage.input_tokens)} />
              <Metric label="输出" value={tokenCount(row.usage.output_tokens)} tone="blue" title={number(row.usage.output_tokens)} />
              <Metric label="缓存命中" value={tokenCount(cached)} tone="green" title={`读取 ${number(cached)} Token`} />
              <Metric label="缓存写入" value={tokenCount(created)} tone="amber" title={`创建 ${number(created)} Token`} />
              <Metric label="命中率" value={cacheRate} />
            </span>
            <span className="log-metric-group" aria-label="耗时与生成速度">
              <Metric label="首字" value={duration(row.first_token_ms)} tone="purple" title="首字耗时（TTFT）；未采集时显示 —" />
              <Metric label="总耗时" value={duration(row.latency_ms)} />
              <Metric label="TPS" value={tps} tone="blue" title="每秒生成 Token 数 = 输出 Token ÷（总耗时 − 首字耗时）；数据不足时显示 —" />
            </span>
          </span>
        </summary>
        <div className="request-log-detail">
          <dl className="log-detail-grid">
            <div><dt>请求 ID</dt><dd className="mono">{row.id}</dd></div>
            <div><dt>请求时间</dt><dd>{new Date(row.timestamp * 1000).toLocaleString("zh-CN", { hour12: false })}</dd></div>
            <div><dt>模型 / 协议</dt><dd>{row.model || "—"} / {row.protocol || "—"}</dd></div>
            <div><dt>请求路径</dt><dd className="mono">{row.request_path || "—"}</dd></div>
            <div><dt>账号</dt><dd>{account}<span className="log-detail-id mono">{row.account_id || "—"}</span></dd></div>
            <div><dt>密钥</dt><dd>{key}<span className="log-detail-id mono">{row.key_id || "—"}</span></dd></div>
            <div><dt>HTTP 状态码</dt><dd>{row.status || "未收到响应"} · {statusLabel}</dd></div>
            <div><dt>上游尝试次数</dt><dd>{number(row.attempts)}</dd></div>
            <div><dt>输入 / 输出 Token</dt><dd>{number(row.usage.input_tokens)} / {number(row.usage.output_tokens)}</dd></div>
            <div><dt>缓存读取 / 写入 Token</dt><dd>{number(cached)} / {number(created)} · 命中率 {cacheRate}</dd></div>
            <div><dt>首字 / 总耗时</dt><dd>{duration(row.first_token_ms)} / {duration(row.latency_ms)}</dd></div>
            <div><dt>生成速度</dt><dd>{tps === "—" ? "—（数据不足）" : `${tps} Token/s`}</dd></div>
            <div><dt>Cyber</dt><dd>{row.cyber ? `已触发${typeof row.cyber === "number" ? ` ${number(row.cyber)} 次` : ""}` : "未触发"}</dd></div>
          </dl>
          {row.error && (
            <div className="log-error-detail">
              <strong>错误原因 / 上游报错</strong>
              <pre>{row.error}</pre>
            </div>
          )}
        </div>
      </details>
    </li>
  );
}

export function RequestLogs({
  revision,
  notify,
  state,
}: {
  revision: number;
  notify: Notice;
  state: State;
}) {
  const [page, setPage] = useState(1);
  const [data, setData] = useState<RequestLogPage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  const [exporting, setExporting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState("");
  const requestSequence = useRef(0);
  useEffect(() => {
    const sequence = ++requestSequence.current;
    setLoading(true);
    setError("");
    void api<RequestLogPage>(`/logs?page=${page}`)
      .then((result) => {
        if (sequence !== requestSequence.current) return;
        const lastPage = Math.max(1, result.total_pages);
        if (page > lastPage || result.page !== page) {
          setPage(Math.min(lastPage, Math.max(1, result.page)));
          return;
        }
        setData(result);
      })
      .catch((failure) => {
        if (sequence === requestSequence.current) setError(errorMessage(failure));
      })
      .finally(() => {
        if (sequence === requestSequence.current) setLoading(false);
      });
    return () => {
      requestSequence.current++;
    };
  }, [page, revision, refresh]);

  async function exportLogs() {
    setExporting(true);
    try {
      const result = await api<RequestLogExport>("/logs/export");
      const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], {
        type: "application/json;charset=utf-8",
      }));
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `lxsh-call-logs-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      notify(`已导出 ${number(result.logs.length)} 条调用日志`);
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setExporting(false);
    }
  }

  async function deleteLogs() {
    setDeleting(true);
    setDeleteError("");
    requestSequence.current++;
    try {
      const result = await api<{ deleted: number }>("/logs", "DELETE");
      setConfirmDelete(false);
      setData(null);
      setPage(1);
      notify(`已删除 ${number(result.deleted)} 条调用日志`);
    } catch (failure) {
      setDeleteError(errorMessage(failure));
    } finally {
      setDeleting(false);
      setRefresh((value) => value + 1);
    }
  }

  const retentionDays = data?.retention_days ?? 7;
  const totalPages = Math.max(1, data?.total_pages ?? 1);
  const currentData = data?.page === page ? data : null;
  const total = data?.total ?? 0;
  const busy = loading || deleting;
  return (
    <>
      <div className="page-heading log-page-heading">
        <div>
          <div className="eyebrow">REQUEST LOGS</div>
          <h1>调用日志<span className="title-count">{number(total)}</span></h1>
          <p>默认保留最近 7 天{retentionDays !== 7 ? ` · 当前保留 ${retentionDays} 天` : ""} · 过期自动清理</p>
        </div>
        <div className="log-page-actions">
          <Button onClick={() => setRefresh((value) => value + 1)} disabled={busy}>
            <RefreshCw size={15} className={loading ? "spin" : ""} />刷新
          </Button>
          <Button onClick={() => void exportLogs()} busy={exporting} disabled={deleting}>
            {!exporting && <Download size={15} />}导出 JSON
          </Button>
          <Button className="danger" disabled={busy || exporting || !total} onClick={() => {
            setDeleteError("");
            setConfirmDelete(true);
          }}>
            <Trash2 size={15} />删除全部
          </Button>
        </div>
      </div>
      <ErrorBox message={error} />
      <section className="request-logs-section" aria-label="调用日志" aria-busy={loading}>
        <div className="log-section-heading">
          <h2><FileClock size={17} />请求记录</h2>
          <span>每页 20 条 · 按时间倒序 · 点击记录展开详情</span>
        </div>
        {loading ? (
          <Empty title="正在加载调用日志" />
        ) : error ? (
          <Empty title="调用日志加载失败" action={<Button onClick={() => setRefresh((value) => value + 1)}>重新加载</Button>} />
        ) : currentData?.items.length ? (
          <ol className="request-log-list">
            {currentData.items.map((row) => <LogRow key={row.id} row={row} state={state} />)}
          </ol>
        ) : (
          <Empty title="暂无调用日志" detail={`最近 ${retentionDays} 天没有调用记录`} />
        )}
        <div className="log-pagination">
          <span className="log-pagination-count" aria-live="polite">
            {currentData?.items.length && !loading
              ? `第 ${(page - 1) * 20 + 1}–${(page - 1) * 20 + currentData.items.length} 条，共 ${number(total)} 条`
              : `共 ${number(total)} 条`}
          </span>
          <div className="log-pagination-controls">
            <Button disabled={busy || !!error || page <= 1} onClick={() => setPage((value) => value - 1)}>
              <ChevronLeft size={15} />上一页
            </Button>
            <label className="log-page-select">
              <span className="visually-hidden">跳转到页码</span>
              <Select aria-label="跳转到页码" value={Math.min(page, totalPages)} disabled={busy || !!error || !total} onChange={(event) => setPage(Number(event.target.value))}>
                {Array.from({ length: totalPages }, (_, index) => <option key={index + 1} value={index + 1}>第 {index + 1} 页 / 共 {totalPages} 页</option>)}
              </Select>
            </label>
            <Button disabled={busy || !!error || page >= totalPages} onClick={() => setPage((value) => value + 1)}>
              下一页<ChevronRight size={15} />
            </Button>
          </div>
        </div>
      </section>
      {confirmDelete && (
        <Confirm title="删除全部调用日志" message="确认删除全部调用日志？此操作不可撤销。需要保留记录时，请先导出 JSON。" busy={deleting} error={deleteError} onClose={() => setConfirmDelete(false)} onConfirm={() => void deleteLogs()} />
      )}
    </>
  );
}
