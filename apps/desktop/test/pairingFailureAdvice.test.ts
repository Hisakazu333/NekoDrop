import assert from "node:assert/strict";
import test from "node:test";

import { pairingFailureAdvice } from "../src/pairingFailureAdvice.ts";

test("turns pairing rejection into a retry hint", () => {
  assert.equal(
    pairingFailureAdvice("对方拒绝配对：用户拒绝配对"),
    "对方拒绝配对；确认配对码后重试"
  );
});

test("turns pairing timeout into a confirm-again hint", () => {
  assert.equal(pairingFailureAdvice("对方拒绝配对：等待确认超时"), "配对超时；让对方重新确认");
});

test("turns pairing code mismatch into a restart hint", () => {
  assert.equal(pairingFailureAdvice("配对码不匹配"), "配对码不匹配；重新发起配对");
});

test("turns offline device pairing failure into a discovery hint", () => {
  assert.equal(
    pairingFailureAdvice("设备不在线或尚未被自动扫描到"),
    "设备离线；刷新附近设备或使用备用码"
  );
});

test("turns receive-disabled pairing failure into a receive hint", () => {
  assert.equal(pairingFailureAdvice("请先打开后台收件，再发起配对。"), "先打开本机收件");
});

test("turns missing identity pairing failure into a broadcast hint", () => {
  assert.equal(
    pairingFailureAdvice("这个设备缺少公开指纹，当前不能发起配对。"),
    "等待对方广播设备身份后重试"
  );
});

test("keeps unknown pairing errors unchanged", () => {
  assert.equal(pairingFailureAdvice("unknown"), null);
  assert.equal(pairingFailureAdvice(null), null);
});
