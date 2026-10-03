import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";
import { copyTextToClipboard } from "../context/helpers";
import { invokeCommand } from "../tauri";
import type { TransferDto } from "../types";

function statusLabel(transfer: TransferDto) {
  const status = transfer.status;
  if (status === "succeeded" || status === "done") return { text: "成功", cls: "is-ok" };
  if (status === "failed") return { text: "失败", cls: "is-failed" };
  if (status === "cancelled") return { text: "已取消", cls: "is-muted" };
  if (status === "transferring" || status === "awaiting_approval") return { text: "进行中", cls: "is-live" };
  return { text: status, cls: "is-muted" };
}

function formatTime(ms: number | null | undefined) {
  if (!ms) return "";
  const date = new Date(Number(ms));
  return `${date.getMonth() + 1}/${date.getDate()} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/**
 * 历史页：行内统计 + 纯列表
 * History page: inline stats + plain rows.
 */
export function HistoryView() {
  const {
    transfers,
    resendTransfer,
    openTransferLocation,
    deleteTransfer,
    clearTransferHistory,
    setToast,
    setError,
    busy
  } = useAppContext();
  const [filter, setFilter] = useState<"all" | "send" | "receive">("all");
  const [query, setQuery] = useState("");
  const [copyingId, setCopyingId] = useState<string | null>(null);

  // 单个 .txt 接收成功记录：一键读回剪贴板（文本快送的接收端闭环）
  const isTextSnippet = (transfer: TransferDto) =>
    transfer.direction === "receive" &&
    (transfer.status === "succeeded" || transfer.status === "done") &&
    transfer.file_count === 1 &&
    (transfer.root_name ?? "").toLowerCase().endsWith(".txt");

  const copyReceivedText = async (transfer: TransferDto) => {
    setCopyingId(transfer.id);
    try {
      const text = await invokeCommand<string>("read_received_text", { transferId: transfer.id });
      await copyTextToClipboard(text);
      setToast?.("文本已复制到剪贴板");
    } catch (error) {
      setError?.(error instanceof Error ? error.message : String(error));
    } finally {
      setCopyingId(null);
    }
  };

  const succeeded = transfers.filter((transfer) => transfer.status === "succeeded" || transfer.status === "done");
  const failed = transfers.filter((transfer) => transfer.status === "failed");
  const totalBytes = succeeded.reduce((sum, transfer) => sum + transfer.total_bytes, 0);
  const visible = transfers.filter((transfer) => {
    if (filter !== "all" && transfer.direction !== filter) return false;
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      (transfer.root_name ?? "").toLowerCase().includes(q) ||
      (transfer.peer_name ?? "").toLowerCase().includes(q) ||
      statusLabel(transfer).text.toLowerCase().includes(q)
    );
  });

  return (
    <div className="page">
      <div className="page-header">
        <h2>历史</h2>
        <p>最近的发送与接收记录</p>
      </div>

      <div className="page-section">
        <div className="stat-line">
          <div className="stat">
            <b>{transfers.length}</b>
            <span>总记录</span>
          </div>
          <div className="stat">
            <b>{succeeded.length}</b>
            <span>成功</span>
          </div>
          <div className="stat">
            <b style={{ color: failed.length ? "var(--danger)" : undefined }}>{failed.length}</b>
            <span>失败</span>
          </div>
          <div className="stat">
            <b>{formatBytes(totalBytes)}</b>
            <span>累计传输</span>
          </div>
        </div>
      </div>

      <div className="page-section">
        <div className="filter-line">
          {(["all", "send", "receive"] as const).map((type) => (
            <button
              className={filter === type ? "is-active" : ""}
              key={type}
              onClick={() => setFilter(type)}
              type="button"
            >
              {type === "all" ? "全部" : type === "send" ? "发送" : "接收"}
            </button>
          ))}
          {transfers.length > 3 && (
            <input
              className="history-search"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索名称 / 对方 / 状态…"
              type="search"
              value={query}
            />
          )}
          {transfers.length > 0 && (
            <button
              className="text-btn is-danger"
              onClick={clearTransferHistory}
              style={{ marginLeft: "auto" }}
              type="button"
            >
              清空历史
            </button>
          )}
        </div>
        <div className="list">
          {visible.length === 0 ? (
            <div className="inline-note">
              {query.trim() ? `没有匹配「${query.trim()}」的记录。` : "还没有传输记录。"}
            </div>
          ) : (
            visible.map((transfer) => {
              const label = statusLabel(transfer);
              return (
                <div className="list-row" key={transfer.id}>
                  <span className="row-icon">
                    <Icon name={transfer.direction === "send" ? "arrow-up" : "upload"} />
                  </span>
                  <div className="row-main">
                    <div className="row-title">{transfer.root_name || "未命名传输"}</div>
                    <div className="row-sub">
                      {transfer.direction === "send" ? "发送" : "接收"}
                      {transfer.peer_name ? ` · ${transfer.peer_name}` : ""} ·{" "}
                      {transfer.file_count} 个文件 · {formatBytes(transfer.total_bytes)} ·{" "}
                      {formatTime(transfer.updated_at_ms ?? null)}
                      {transfer.error_message ? ` · ${transfer.error_message}` : ""}
                    </div>
                  </div>
                  <span className={`state-tag ${label.cls}`}>{label.text}</span>
                  <div className="row-ops">
                    {isTextSnippet(transfer) && (
                      <button
                        className="icon-btn"
                        disabled={busy === "open" || copyingId === transfer.id}
                        onClick={() => void copyReceivedText(transfer)}
                        title="复制文本到剪贴板"
                        type="button"
                      >
                        <Icon name={copyingId === transfer.id ? "check" : "copy"} />
                      </button>
                    )}
                    <button
                      className="icon-btn"
                      onClick={() => openTransferLocation(transfer)}
                      title="打开位置"
                      type="button"
                    >
                      <Icon name="folder" />
                    </button>
                    <button
                      className="icon-btn"
                      onClick={() => resendTransfer(transfer)}
                      title="重新发送"
                      type="button"
                    >
                      <Icon name="refresh" />
                    </button>
                    <button
                      className="icon-btn"
                      onClick={() => deleteTransfer(transfer)}
                      title="删除记录"
                      type="button"
                    >
                      <Icon name="trash" />
                    </button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
