export function transferFailureAdvice(message: string | null | undefined) {
  const text = message?.trim();
  if (!text) return null;

  const lower = text.toLowerCase();

  if (includesAny(lower, ["transfer cancelled", "传输已取消", "已取消"])) return null;
  if (includesAny(lower, ["receiver declined", "transfer declined", "对方拒绝"])) return null;

  if (includesAny(lower, ["insufficient receive space", "disk full", "空间不足"])) {
    return "清理接收端磁盘空间，或更换接收目录";
  }

  if (includesAny(lower, ["checksum", "sha-256", "sha256", "does not match accepted offer", "校验失败", "不一致"])) {
    return "重新发送该文件";
  }

  if (
    includesAny(lower, [
      "transport is not available",
      "unsupported transport",
      "requested iroh",
      "requested relay",
      "requested quic",
      "传输通道",
      "没有接入"
    ])
  ) {
    return "改用局域网设备或备用码";
  }

  if (
    includesAny(lower, [
      "timed out",
      "timeout",
      "连接超时",
      "连接尝试失败",
      "没有正确答复"
    ])
  ) {
    return "确认同一局域网；关闭 VPN/代理；可用备用码";
  }

  if (
    includesAny(lower, [
      "connection refused",
      "actively refused",
      "connection reset",
      "failed to connect",
      "无法连接对方电脑",
      "积极拒绝"
    ])
  ) {
    return "确认对方已打开收件；Windows 允许专用网络";
  }

  if (
    includesAny(lower, [
      "network is unreachable",
      "no route to host",
      "host unreachable",
      "无法到达",
      "无法访问目标主机"
    ])
  ) {
    return "确认同一局域网；可用备用码";
  }

  if (includesAny(lower, ["permission denied", "access is denied", "operation not permitted", "权限"])) {
    return "检查目录权限，或重新选择可写接收目录";
  }

  if (includesAny(lower, ["no such file", "not found", "路径不存在", "文件或目录不存在"])) {
    return "重新选择源文件";
  }

  if (includesAny(lower, ["127.0.0.1", "localhost", "指向了本机"])) {
    return "重新复制对方连接码，或从附近设备发送";
  }

  if (includesAny(lower, ["198.18.", "198.19.", "代理", "vpn", "虚拟网卡"])) {
    return "关闭代理/VPN，或改用真实局域网地址";
  }

  return null;
}

function includesAny(text: string, needles: string[]) {
  return needles.some((needle) => text.includes(needle));
}
