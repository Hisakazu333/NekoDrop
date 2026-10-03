import { useState } from "react";
import { invokeCommand } from "../tauri";
import { keepIfEqual, errorMessage, normalizeReceivePolicy, type BusyMode, type ReceivePolicyMode } from "./helpers";
import type { AppSnapshot, ReceivePortDiagnosticsDto } from "../types";

interface SettingsDeps {
  setBusy: (mode: BusyMode | null) => void;
  setError: (message: string | null) => void;
  setToast: (message: string | null) => void;
  refreshReceiveState: (options?: { includeDiagnostics?: boolean }) => Promise<void>;
  receiveSession: import("../types").ReceiveSessionDto | null;
  parseReceivePortValue: (value: string) => number | null;
}

export function useSettingsDomain(deps: SettingsDeps) {
  const { setBusy, setError, setToast, refreshReceiveState, receiveSession, parseReceivePortValue } = deps;
  const [deviceNameInput, setDeviceNameInput] = useState("这台电脑");

  const [receivePolicy, setReceivePolicy] = useState<ReceivePolicyMode>("always_ask");

  const [bindPort, setBindPort] = useState("45821");

  const [sendLimitInput, setSendLimitInput] = useState("0");

  const [receiveDir, setReceiveDir] = useState("~/Downloads/NekoDrop");

  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);

  async function refreshSnapshot() {
    const nextSnapshot = await invokeCommand<AppSnapshot>("get_app_snapshot");
    setSnapshot(nextSnapshot);
    setDeviceNameInput(nextSnapshot.device_name);
    setReceiveDir(nextSnapshot.receive_dir);
    setBindPort(String(nextSnapshot.receive_port));
    setSendLimitInput(String(nextSnapshot.send_limit_kbps ?? 0));
    setReceivePolicy(normalizeReceivePolicy(nextSnapshot.receive_policy));
  }

  async function chooseReceiveDir() {
    setBusy("pick-receive");
    setError(null);
    try {
      const pickedDir = await invokeCommand<string | null>("select_receive_dir");
      if (pickedDir) {
        await invokeCommand<void>("set_receive_dir", { receiveDir: pickedDir });
        setReceiveDir(pickedDir);
        await refreshSnapshot();
        setToast("接收目录已更新");
      }
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveReceiveDir() {
    if (receiveSession) return;
    const nextReceiveDir = receiveDir.trim();
    if (!nextReceiveDir || nextReceiveDir === snapshot?.receive_dir) return;
    setBusy("pick-receive");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_dir", { receiveDir: nextReceiveDir });
      setReceiveDir(nextReceiveDir);
      setSnapshot((current) => (current ? { ...current, receive_dir: nextReceiveDir } : current));
      await refreshSnapshot();
      setToast("接收目录已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveReceivePort() {
    if (receiveSession) return;
    const nextReceivePort = parseReceivePortValue(bindPort);
    if (nextReceivePort === null || nextReceivePort === snapshot?.receive_port) return;
    setBusy("pick-receive");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_port", { receivePort: nextReceivePort });
      setBindPort(String(nextReceivePort));
      setSnapshot((current) => (current ? { ...current, receive_port: nextReceivePort } : current));
      setToast("默认端口已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveSendLimit() {
    const nextLimit = Number.parseInt(sendLimitInput.trim(), 10);
    if (!Number.isFinite(nextLimit) || nextLimit < 0 || nextLimit > 1_000_000) {
      setError("限速需为 0-1000000 KB/s（0 = 不限）");
      return;
    }
    if (nextLimit === snapshot?.send_limit_kbps) return;
    setBusy("pick-receive");
    setError(null);
    try {
      await invokeCommand<void>("set_send_limit", { sendLimitKbps: nextLimit });
      setSendLimitInput(String(nextLimit));
      setSnapshot((current) => (current ? { ...current, send_limit_kbps: nextLimit } : current));
      setToast(nextLimit === 0 ? "已解除发送限速" : `发送限速：${nextLimit} KB/s`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function updateReceivePolicy(nextPolicy: ReceivePolicyMode) {
    if (nextPolicy === receivePolicy) return;
    setBusy("receive-policy");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_policy", { receivePolicy: nextPolicy });
      setReceivePolicy(nextPolicy);
      setSnapshot((current) => (current ? { ...current, receive_policy: nextPolicy } : current));
      setToast(nextPolicy === "block_all" ? "接收策略：阻止" : "接收策略：询问");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveDeviceName() {
    const nextName = deviceNameInput.trim();
    if (!nextName || nextName === snapshot?.device_name) return;
    setBusy("device-name");
    setError(null);
    try {
      const savedName = await invokeCommand<string>("set_device_name", { deviceName: nextName });
      setSnapshot((current) =>
        current
          ? {
              ...current,
              device_name: savedName,
              device_identity: { ...current.device_identity, device_name: savedName }
            }
          : current
      );
      setDeviceNameInput(savedName);
      setToast("设备名已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function openPath(path: string) {
    setBusy("open");
    setError(null);
    try {
      await invokeCommand<void>("open_path", { path });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }
  return {
    snapshot, receiveDir, bindPort, receivePolicy, deviceNameInput,
    setReceiveDir, setBindPort, setReceivePolicy, setDeviceNameInput,
    refreshSnapshot, chooseReceiveDir, saveReceiveDir, saveReceivePort, sendLimitInput, setSendLimitInput, saveSendLimit,
    updateReceivePolicy, saveDeviceName, openPath,
  };
}

