import assert from "node:assert/strict";
import { test } from "node:test";

import { transferFailureAdvice } from "../src/transferFailureAdvice.ts";

test("suggests receive and firewall checks for refused connections", () => {
  const advice = transferFailureAdvice(
    "无法连接对方电脑。请确认对方 NekoDrop 正在运行、收件已开启、防火墙允许访问，且两台设备网络互通。"
  );

  assert.equal(advice, "确认对方已打开收件；Windows 允许专用网络");
});

test("suggests same-network checks for connection timeouts", () => {
  const advice = transferFailureAdvice(
    "连接超时。常见原因是 Windows 防火墙拦截、两台设备不在同一网段、路由器隔离了有线/无线，或 VPN/代理影响了局域网连接。"
  );

  assert.equal(advice, "确认同一局域网；关闭 VPN/代理；可用备用码");
});

test("suggests storage and retry actions for common transfer failures", () => {
  assert.equal(
    transferFailureAdvice("接收目录所在磁盘空间不足。请清理空间，或在设置里选择另一个接收目录后重试。"),
    "清理接收端磁盘空间，或更换接收目录"
  );
  assert.equal(transferFailureAdvice("文件校验失败，已拒绝把不一致的内容当作完成文件。请重新发送。"), "重新发送该文件");
  assert.equal(transferFailureAdvice("当前版本还没有接入这个传输通道。请先使用局域网自动发现或连接码兜底。"), "改用局域网设备或备用码");
});

test("returns no advice for cancelled or empty messages", () => {
  assert.equal(transferFailureAdvice("传输已取消"), null);
  assert.equal(transferFailureAdvice(null), null);
  assert.equal(transferFailureAdvice(""), null);
});
