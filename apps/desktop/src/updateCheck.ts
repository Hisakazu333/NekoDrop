/**
 * 应用内更新检查：读 GitHub Releases 最新 tag 与当前版本比较。
 * 只做提示不自动下载（自动更新需要签名密钥，后续接 tauri updater）。
 */

export interface UpdateInfo {
  tagName: string;
  url: string;
}

const CURRENT_VERSION = "0.1.1";
const RELEASES_API = "https://api.github.com/repos/Hisakazu333/NekoDrop/releases/latest";
const RELEASES_PAGE = "https://github.com/Hisakazu333/NekoDrop/releases/latest";

/** 语义比较：v0.1.10 > v0.1.9 */
export function isNewerVersion(candidate: string, current: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/, "")
      .split(".")
      .map((part) => Number.parseInt(part, 10) || 0);
  const [cMajor, cMinor, cPatch] = parse(candidate);
  const [mMajor, mMinor, mPatch] = parse(current);
  if (cMajor !== mMajor) return cMajor > mMajor;
  if (cMinor !== mMinor) return cMinor > mMinor;
  return cPatch > mPatch;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const response = await fetch(RELEASES_API, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) {
    throw new Error(`更新检查失败：HTTP ${response.status}`);
  }
  const release = (await response.json()) as {
    tag_name?: string;
    html_url?: string;
    draft?: boolean;
    prerelease?: boolean;
  };
  const tagName = release.tag_name ?? "";
  if (!tagName || release.draft || release.prerelease) return null;
  if (!isNewerVersion(tagName, CURRENT_VERSION)) return null;
  return {
    tagName,
    url: release.html_url || RELEASES_PAGE,
  };
}
