import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";

/**
 * 全屏传输历史管理组件
 * Transfers History Manager Component
 */
export function TransfersManager() {
  const { transfers, deleteTransfer, resendTransfer, openTransferLocation, clearTransferHistory, busy } =
    useAppContext();

  const [filterType, setFilterType] = useState<"all" | "send" | "receive">("all");

  const filteredTransfers = transfers.filter((t) => {
    if (filterType === "all") return true;
    return t.direction === filterType;
  });

  const totalCount = transfers.length;
  const successCount = transfers.filter((t) => t.status === "completed").length;
  const failedCount = totalCount - successCount;

  return (
    <div className="manager-pane transfers-manager">
      <div className="manager-header is-row">
        <div>
          <h2>历史传输记录</h2>
          <p>查看过去的文件传输记录，并按方向筛选、重发或清理。</p>
        </div>
        {totalCount > 0 && (
          <button className="btn-danger-action" onClick={clearTransferHistory} disabled={busy === "history"}>
            <Icon name="trash" />
            <span>清空全部</span>
          </button>
        )}
      </div>

      <div className="manager-body">
        {/* 历史统计卡片区 / History Stats Cards */}
        {totalCount > 0 && (
          <div className="stats-cards-grid">
            <div className="stat-card">
              <span className="stat-label">总传输次数</span>
              <strong className="stat-val">{totalCount}</strong>
            </div>
            <div className="stat-card">
              <span className="stat-label">传输成功</span>
              <strong className="stat-val text-success">{successCount}</strong>
            </div>
            <div className="stat-card">
              <span className="stat-label">传输失败</span>
              <strong className="stat-val text-danger">{failedCount}</strong>
            </div>
          </div>
        )}

        {/* 过滤页签 / Filter Tabs */}
        <div className="list-filter-bar">
          <button
            className={`filter-btn ${filterType === "all" ? "is-active" : ""}`}
            onClick={() => setFilterType("all")}
            type="button"
          >
            全部
          </button>
          <button
            className={`filter-btn ${filterType === "send" ? "is-active" : ""}`}
            onClick={() => setFilterType("send")}
            type="button"
          >
            发送
          </button>
          <button
            className={`filter-btn ${filterType === "receive" ? "is-active" : ""}`}
            onClick={() => setFilterType("receive")}
            type="button"
          >
            接收
          </button>
        </div>

        {/* 历史记录表格 / History Table */}
        {filteredTransfers.length > 0 ? (
          <div className="table-wrapper">
            <table className="standard-table">
              <thead>
                <tr>
                  <th>传输内容</th>
                  <th>方向</th>
                  <th>对端节点</th>
                  <th>容量</th>
                  <th>状态</th>
                  <th>时间</th>
                  <th className="align-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {filteredTransfers.map((transfer) => {
                  const isSuccess = transfer.status === "completed";
                  const isSend = transfer.direction === "send";
                  const dateStr = new Date(transfer.created_at_ms).toLocaleString();

                  return (
                    <tr key={transfer.id}>
                      <td>
                        <div className="table-file-cell">
                          <Icon
                            name={transfer.file_count > 1 ? "package" : "file"}
                            className="file-cell-icon"
                          />
                          <strong className="file-cell-name" title={transfer.root_name}>
                            {transfer.root_name}
                          </strong>
                        </div>
                      </td>
                      <td>
                        <span className={`direction-badge ${isSend ? "is-send" : "is-receive"}`}>
                          {isSend ? "发送" : "接收"}
                        </span>
                      </td>
                      <td>{transfer.peer_name || "未知设备"}</td>
                      <td>{formatBytes(transfer.total_bytes)}</td>
                      <td>
                        <span className={`status-text-tag ${isSuccess ? "is-success" : "is-failed"}`}>
                          {isSuccess ? "● 成功" : "● 失败"}
                        </span>
                      </td>
                      <td className="time-col">{dateStr}</td>
                      <td className="align-right">
                        <div className="table-ops-group">
                          <button
                            className="btn-table-icon-op"
                            onClick={() => openTransferLocation(transfer)}
                            title="打开所在文件夹"
                            type="button"
                          >
                            <Icon name="folder" />
                          </button>
                          {!isSuccess && isSend && (
                            <button
                              className="btn-table-icon-op btn-retry"
                              onClick={() => resendTransfer(transfer)}
                              title="重新发送"
                              type="button"
                            >
                              <Icon name="refresh" />
                            </button>
                          )}
                          <button
                            className="btn-table-icon-op btn-delete"
                            onClick={() => deleteTransfer(transfer)}
                            title="删除记录"
                            type="button"
                          >
                            <Icon name="trash" />
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="manager-empty-state">
            <Icon name="clock" className="empty-icon" />
            <p>暂无符合筛选条件的历史传输记录。</p>
          </div>
        )}
      </div>
    </div>
  );
}
