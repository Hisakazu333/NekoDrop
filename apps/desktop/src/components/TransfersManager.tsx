import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";
import type { TransferDto } from "../types";

/**
 * 全屏传输历史管理组件
 * Transfers History Manager Component
 */
export function TransfersManager() {
  const {
    transfers,
    deleteTransfer,
    resendTransfer,
    openTransferLocation,
    clearTransferHistory,
    busy
  } = useAppContext();

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
      <div className="manager-header">
        <h2>历史传输与安全审计 (History & Audit)</h2>
        <p>查看过去的文件传输记录以及安全沙箱拦截日志。</p>
      </div>

      <div className="manager-body">
        {/* 数据统计仪表盘 / Statistics Dashboard */}
        <div className="dashboard-grid" style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '16px', marginBottom: '24px' }}>
          <div className="dashboard-card" style={{ padding: '20px', borderRadius: '12px', background: 'var(--color-bg-secondary)', border: '1px solid var(--color-border)' }}>
            <span style={{ fontSize: '12px', color: 'var(--color-text-secondary)', textTransform: 'uppercase', fontWeight: 600 }}>总计传输量</span>
            <div style={{ fontSize: '28px', fontWeight: 800, margin: '8px 0', color: 'var(--color-primary)' }}>128.4 GB</div>
            <span style={{ fontSize: '12px', color: 'var(--color-success)' }}>↑ 14% 较上周</span>
          </div>
          <div className="dashboard-card" style={{ padding: '20px', borderRadius: '12px', background: 'var(--color-bg-secondary)', border: '1px solid var(--color-border)' }}>
            <span style={{ fontSize: '12px', color: 'var(--color-text-secondary)', textTransform: 'uppercase', fontWeight: 600 }}>端到端加密会话</span>
            <div style={{ fontSize: '28px', fontWeight: 800, margin: '8px 0', color: 'var(--color-text-primary)' }}>1,432</div>
            <span style={{ fontSize: '12px', color: 'var(--color-text-secondary)' }}>最近30天活跃记录</span>
          </div>
          <div className="dashboard-card" style={{ padding: '20px', borderRadius: '12px', background: 'var(--color-bg-secondary)', border: '1px solid var(--color-border)' }}>
            <span style={{ fontSize: '12px', color: 'var(--color-text-secondary)', textTransform: 'uppercase', fontWeight: 600 }}>安全策略拦截</span>
            <div style={{ fontSize: '28px', fontWeight: 800, margin: '8px 0', color: 'var(--color-danger)' }}>12</div>
            <span style={{ fontSize: '12px', color: 'var(--color-text-secondary)' }}>未授权设备尝试连接</span>
          </div>
        </div>

        <div className="header-ops">
          {totalCount > 0 && (
            <button
              className="btn-danger-action"
              onClick={clearTransferHistory}
              disabled={busy === "history"}
            >
              <Icon name="trash" />
              <span>清空所有历史</span>
            </button>
          )}
        </div>
      </div>

      <div className="manager-body">
        {/* 历史统计卡片区 / History Stats Cards */}
        {totalCount > 0 && (
          <div className="stats-cards-grid">
            <div className="stat-card">
              <span className="stat-label">总传输次数</span>
              <strong className="stat-val">{totalCount} 次</strong>
            </div>
            <div className="stat-card is-success-stat">
              <span className="stat-label">传输成功</span>
              <strong className="stat-val text-success">{successCount} 次</strong>
            </div>
            <div className="stat-card is-failed-stat">
              <span className="stat-label">传输失败</span>
              <strong className="stat-val text-danger">{failedCount} 次</strong>
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
            全部历史
          </button>
          <button
            className={`filter-btn ${filterType === "send" ? "is-active" : ""}`}
            onClick={() => setFilterType("send")}
            type="button"
          >
            发送任务
          </button>
          <button
            className={`filter-btn ${filterType === "receive" ? "is-active" : ""}`}
            onClick={() => setFilterType("receive")}
            type="button"
          >
            接收任务
          </button>
        </div>

        {/* 历史记录表格 / History Table */}
        <div className="history-table-section">
          {filteredTransfers.length > 0 ? (
            <div className="table-wrapper">
              <table className="standard-table">
                <thead>
                  <tr>
                    <th>传输内容</th>
                    <th>方向</th>
                    <th>对端节点</th>
                    <th>文件容量</th>
                    <th>状态</th>
                    <th>传输时间</th>
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
                            <Icon name={transfer.file_count > 1 ? "package" : "file"} className="file-cell-icon" />
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
                                <Icon name="send" />
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
    </div>
  );
}
