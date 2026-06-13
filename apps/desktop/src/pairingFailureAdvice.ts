import { transferFailureAdvice } from "./transferFailureAdvice.ts";

export function pairingFailureAdvice(message: string | null | undefined) {
  const text = message?.trim();
  if (!text) return null;

  if (includesAny(text, ["等待确认超时", "配对超时"])) {
    return "配对超时；让对方重新确认";
  }

  if (includesAny(text, ["配对码不匹配"])) {
    return "配对码不匹配；重新发起配对";
  }

  if (includesAny(text, ["用户拒绝配对", "对方拒绝配对", "配对被拒绝"])) {
    return "对方拒绝配对；确认配对码后重试";
  }

  if (includesAny(text, ["设备不在线或尚未被自动扫描到"])) {
    return "设备离线；刷新附近设备或使用备用码";
  }

  if (includesAny(text, ["请先打开后台收件"])) {
    return "先打开本机收件";
  }

  if (includesAny(text, ["缺少公开指纹", "不能发起配对"])) {
    return "等待对方广播设备身份后重试";
  }

  return transferFailureAdvice(text);
}

function includesAny(text: string, needles: string[]) {
  return needles.some((needle) => text.includes(needle));
}
