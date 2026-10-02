import { useState } from "react";
import { invokeCommand } from "../tauri";
import { keepIfEqual, errorMessage, type BusyMode } from "./helpers";
import { localBridgeStatusLabel } from "../localBridgeState";
import type {
  LocalBridgeRuntimeStatusDto,
  LocalBridgeAuthorizationDto,
  LocalBridgeAuthorizationListDto,
  LocalBridgePendingActionDto,
  LocalBridgePendingActionListDto,
  LocalBridgePendingActionResultListDto,
  LocalBridgePendingActionResultDto,
  LocalBridgeResponseDto,
} from "../types";

interface BridgeDeps {
  setBusy: (mode: BusyMode | null) => void;
  setError: (message: string | null) => void;
  setToast: (message: string | null) => void;
  refreshDirectoryState: () => Promise<void>;
}

export function useBridgeDomain(deps: BridgeDeps) {
  const { setBusy, setError, setToast, refreshDirectoryState } = deps;
  const [localBridgeAuthorizationCode, setLocalBridgeAuthorizationCode] = useState("");

  const [localBridgeCheck, setLocalBridgeCheck] = useState<string | null>(null);

  const [localBridgeActionResults, setLocalBridgeActionResults] = useState<LocalBridgePendingActionResultDto[]>([]);

  const [localBridgePendingActions, setLocalBridgePendingActions] = useState<LocalBridgePendingActionDto[]>([]);

  const [localBridgeAuthorizations, setLocalBridgeAuthorizations] = useState<LocalBridgeAuthorizationDto[]>([]);

  const [localBridgeStatus, setLocalBridgeStatus] = useState<LocalBridgeRuntimeStatusDto | null>(null);

  async function refreshLocalBridgeStatus() {
    const status = await invokeCommand<LocalBridgeRuntimeStatusDto>("get_local_bridge_runtime_status");
    setLocalBridgeStatus((current) => keepIfEqual(current, status));
  }

  async function refreshLocalBridgeAuthorizations() {
    const response = await invokeCommand<LocalBridgeAuthorizationListDto>("list_local_bridge_authorizations");
    setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
  }

  async function refreshLocalBridgePendingActions() {
    const response = await invokeCommand<LocalBridgePendingActionListDto>("list_local_bridge_pending_actions");
    setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
  }

  async function refreshLocalBridgeActionResults() {
    const response = await invokeCommand<LocalBridgePendingActionResultListDto>("list_local_bridge_pending_action_results");
    setLocalBridgeActionResults((current) => keepIfEqual(current, response.results));
  }

  async function runLocalBridgeSelfCheck() {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgeResponseDto>("handle_local_bridge_request", {
        requestJson: JSON.stringify({
          "kind": "devices.list",
          "payload": {
            request_id: `settings-self-check-${Date.now()}`,
            trusted_only: true,
            client: {
              client_id: "nekodrop.settings",
              display_name: "NekoDrop Settings"
            }
          }
        })
      });
      setLocalBridgeCheck(
        response.authorization_code
          ? `${localBridgeStatusLabel(response.status)} · 授权码 ${response.authorization_code}`
          : `${localBridgeStatusLabel(response.status)} · ${response.devices.length} 台可信设备 · ${response.staged_bundles.length} 个暂存资料包`
      );
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setLocalBridgeCheck("自测失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function confirmLocalBridgeAuthorization() {
    const code = localBridgeAuthorizationCode.trim();
    if (!code) {
      setLocalBridgeCheck("请输入授权码");
      return;
    }
    setBusy("open");
    setError(null);
    try {
      const authorization = await invokeCommand<LocalBridgeAuthorizationDto>("confirm_local_bridge_authorization", {
        authorizationCode: code
      });
      setLocalBridgeAuthorizationCode("");
      setLocalBridgeCheck(`已授权 ${authorization.display_name}`);
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeAuthorizations();
    } catch (nextError) {
      setLocalBridgeCheck("授权失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function removeLocalBridgePendingAction(action: LocalBridgePendingActionDto) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<{ actions: LocalBridgePendingActionDto[]; removed: boolean }>("remove_local_bridge_pending_action", {
        requestId: action.request_id
      });
      setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
      setToast("已处理该请求");
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function respondLocalBridgePendingAction(action: LocalBridgePendingActionDto, accept: boolean) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<{
        handled: boolean;
        accepted: boolean;
        actions: LocalBridgePendingActionDto[];
      }>("respond_local_bridge_pending_action", {
        requestId: action.request_id,
        accept
      });
      setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
      setToast(accept ? "已允许并执行该请求" : "已拒绝该请求");
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeActionResults();
      await refreshDirectoryState();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function revokeLocalBridgeAuthorization(authorization: LocalBridgeAuthorizationDto, scope: string) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<{ authorizations: LocalBridgeAuthorizationDto[]; revoked: boolean }>("revoke_local_bridge_authorization", {
        clientId: authorization.client_id,
        scope
      });
      setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
      setToast("已撤销授权");
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function pruneLocalBridgeAuthorizations() {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgeAuthorizationListDto>("prune_local_bridge_authorizations");
      setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
      setLocalBridgeCheck(response.pruned_count > 0 ? `已清理 ${response.pruned_count} 条过期授权` : "没有过期授权");
      setToast("已清理过期授权");
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeActionResults();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }
  return {
    localBridgeStatus, setLocalBridgeStatus,
    localBridgeAuthorizations, setLocalBridgeAuthorizations,
    localBridgePendingActions, setLocalBridgePendingActions,
    localBridgeActionResults, setLocalBridgeActionResults,
    localBridgeCheck, setLocalBridgeCheck,
    localBridgeAuthorizationCode, setLocalBridgeAuthorizationCode,
    refreshLocalBridgeStatus, refreshLocalBridgeAuthorizations,
    refreshLocalBridgePendingActions, refreshLocalBridgeActionResults,
    runLocalBridgeSelfCheck, confirmLocalBridgeAuthorization,
    removeLocalBridgePendingAction, respondLocalBridgePendingAction,
    revokeLocalBridgeAuthorization, pruneLocalBridgeAuthorizations,
  };
}

