import { useRef, useState } from "react";
import { invokeCommand } from "../tauri";
import { buildPathPayload, errorMessage, lastPathSegment, uniquePaths, type BusyMode, type ComposerMode } from "./helpers";
import type { TransferPlanDto, TransferScanProgressDto, ManualBundleCreateDto } from "../types";

interface ComposerDeps {
  setBusy: (mode: BusyMode | null) => void;
  setError: (message: string | null) => void;
  setToast: (message: string | null) => void;
  setMode: (mode: ComposerMode) => void;
  setSendReport: (report: import("../types").SendReportDto | null) => void;
}

export function useComposerDomain(deps: ComposerDeps) {
  const { setBusy, setError, setToast, setMode, setSendReport } = deps;
  const [createdManualBundle, setCreatedManualBundle] = useState<ManualBundleCreateDto | null>(null);
  const scanSeq = useRef(0);
  const [scanStatus, setScanStatus] = useState<TransferScanProgressDto | null>(null);

  const [plan, setPlan] = useState<TransferPlanDto | null>(null);

  const [manualBundleSourcePath, setManualBundleSourcePath] = useState("");

  const [manualBundleSourceApp, setManualBundleSourceApp] = useState("NekoDrop");

  const [manualBundleDisplayName, setManualBundleDisplayName] = useState("");

  const [manualBundleType, setManualBundleType] = useState("workspace");

  const [manualPaths, setManualPaths] = useState("");

  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);

  async function pickFiles() {
    setBusy("pick-files");
    setError(null);
    try {
      const paths = await invokeCommand<string[]>("select_send_files");
      await applyPickedPaths(paths);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function pickFolders() {
    setBusy("pick-folders");
    setError(null);
    try {
      const paths = await invokeCommand<string[]>("select_send_folders");
      await applyPickedPaths(paths);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function chooseManualBundleSourceDir() {
    setBusy("pick-folders");
    setError(null);
    try {
      const picked = await invokeCommand<string | null>("select_manual_bundle_source_dir");
      if (!picked) return;
      setManualBundleSourcePath(picked);
      if (!manualBundleDisplayName.trim()) {
        setManualBundleDisplayName(lastPathSegment(picked));
      }
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function createManualBundleForSend() {
    const sourcePath = manualBundleSourcePath.trim();
    if (!sourcePath) {
      setError("选择来源目录");
      return;
    }
    setBusy("scan");
    setError(null);
    setCreatedManualBundle(null);
    try {
      const bundle = await invokeCommand<ManualBundleCreateDto>("create_manual_bundle", {
        request: {
          source_path: sourcePath,
          bundle_type: manualBundleType,
          display_name: manualBundleDisplayName.trim() || lastPathSegment(sourcePath),
          source_app: manualBundleSourceApp.trim() || "NekoDrop"
        }
      });
      setCreatedManualBundle(bundle);
      setToast(`已创建资料包：${bundle.display_name}`);
      await applyPickedPaths([bundle.staging_path]);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function applyPickedPaths(paths: string[]) {
    if (paths.length === 0) return;
    const mergedPaths = uniquePaths([...selectedPaths, ...paths]);
    setSelectedPaths(mergedPaths);
    setSendReport(null);
    setMode("send");
    setToast(`已加入 ${paths.length} 个路径`);
    await scanPaths(mergedPaths, manualPaths);
  }

  function removePath(path: string) {
    const nextPaths = selectedPaths.filter((item) => item !== path);
    setSelectedPaths(nextPaths);
    setPlan(null);
    setScanStatus(null);
    setSendReport(null);
  }

  function clearQueue() {
    setSelectedPaths([]);
    setManualPaths("");
    setPlan(null);
    setScanStatus(null);
    setSendReport(null);
  }

  async function scanPaths(paths = selectedPaths, manual = manualPaths) {
    const payload = buildPathPayload(paths, manual);
    if (payload.length === 0) return;

    // Only the most recent scan may commit its plan or clear busy/scanStatus:
    // a slow earlier scan must not overwrite a newer result or un-busy the
    // UI while the newer scan is still running.
    const requestId = ++scanSeq.current;
    setBusy("scan");
    setError(null);
    setScanStatus(null);
    setSendReport(null);
    try {
      const nextPlan = await invokeCommand<TransferPlanDto>("create_transfer_plan", { paths: payload });
      if (requestId !== scanSeq.current) return;
      setPlan(nextPlan);
    } catch (nextError) {
      if (requestId === scanSeq.current) setError(errorMessage(nextError));
    } finally {
      if (requestId === scanSeq.current) {
        setScanStatus(null);
        setBusy(null);
      }
    }
  }
  return {
    selectedPaths, setSelectedPaths, manualPaths, setManualPaths,
    manualBundleType, setManualBundleType, manualBundleDisplayName, setManualBundleDisplayName,
    manualBundleSourceApp, setManualBundleSourceApp, manualBundleSourcePath, setManualBundleSourcePath,
    plan, setPlan, scanStatus, setScanStatus,
    createdManualBundle, setCreatedManualBundle,
    pickFiles, pickFolders, chooseManualBundleSourceDir, createManualBundleForSend,
    applyPickedPaths, removePath, clearQueue, scanPaths,
  };
}

