import { useState } from "react";
import { invokeCommand } from "../tauri";
import { keepIfEqual, errorMessage, type BusyMode } from "./helpers";
import { bundleImportStrategyLabel, markBundleDeleted, markBundleImportFailed } from "../bundleState";
import type { ReceivedBundleDto, ReceiveReportDto } from "../types";

interface InboxDeps {
  setBusy: (mode: BusyMode | null) => void;
  setError: (message: string | null) => void;
  setToast: (message: string | null) => void;
  setReceiveReport: (updater: (current: ReceiveReportDto | null) => ReceiveReportDto | null) => void;
  refreshDirectoryState: () => Promise<void>;
  refreshReceiveState: (options?: { includeDiagnostics?: boolean }) => Promise<void>;
}

export function useInboxDomain(deps: InboxDeps) {
  const { setBusy, setError, setToast, setReceiveReport, refreshDirectoryState, refreshReceiveState } = deps;
  const [stagedBundles, setStagedBundles] = useState<ReceivedBundleDto[]>([]);

  async function importCurrentStagedBundle(bundle: ReceivedBundleDto, conflictStrategy = "reject") {
    setBusy("bundle-import");
    setError(null);
    try {
      const imported = await invokeCommand<ReceivedBundleDto>("import_staged_bundle", {
        request: {
          bundle_id: bundle.bundle_id,
          conflict_strategy: conflictStrategy
        }
      });
      setStagedBundles((current) => current.map((item) => (item.bundle_id === imported.bundle_id ? imported : item)));
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: imported };
      });
      const strategyLabel = imported.imported_with_strategy
        ? ` · ${bundleImportStrategyLabel(imported.imported_with_strategy)}`
        : "";
      const skipped = imported.import_skipped_file_count > 0 ? `，跳过 ${imported.import_skipped_file_count} 个` : "";
      setToast(`已导入：${imported.display_name}${skipped}${strategyLabel}`);
    } catch (nextError) {
      setStagedBundles((current) =>
        current.map((item) => (item.bundle_id === bundle.bundle_id ? markBundleImportFailed(item) : item))
      );
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: markBundleImportFailed(current.bundle) };
      });
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function rollbackCurrentBundle(bundle: ReceivedBundleDto) {
    setBusy("bundle-import");
    setError(null);
    try {
      const rolledBack = await invokeCommand<ReceivedBundleDto>("rollback_imported_bundle", {
        request: {
          bundle_id: bundle.bundle_id
        }
      });
      setStagedBundles((current) =>
        current.map((item) => (item.bundle_id === bundle.bundle_id ? rolledBack : item))
      );
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: rolledBack };
      });
      setToast(`已撤回：${rolledBack.display_name}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function deleteCurrentStagedBundle(bundle: ReceivedBundleDto) {
    setBusy("receive");
    setError(null);
    try {
      await invokeCommand<boolean>("delete_staged_bundle", { bundleId: bundle.bundle_id });
      setStagedBundles((current) => current.filter((item) => item.bundle_id !== bundle.bundle_id));
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: markBundleDeleted(current.bundle) };
      });
      setToast("已删除暂存资料包");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }
  return {
    stagedBundles, setStagedBundles,
    importCurrentStagedBundle, rollbackCurrentBundle, deleteCurrentStagedBundle,
  };
}

