// 等待公共 registry 可见，避免主包先于原生依赖可安装。
export async function releaseVisible(manifest, request = fetch) {
  const url = `https://registry.npmjs.org/${encodeURIComponent(manifest.name)}/${manifest.version}`;
  const response = await request(url, { headers: { "cache-control": "no-cache" }, signal: AbortSignal.timeout(15000) });
  if (response.status === 404) return false;
  if (!response.ok) throw new Error(`registry 返回 HTTP ${response.status}，停止发布`);
  const value = await response.json();
  if (value.name !== manifest.name || value.version !== manifest.version || !value.dist?.integrity) {
    throw new Error(`registry 发布元数据无效：${manifest.name}`);
  }
  return true;
}

export async function waitForRegistry(manifests, { request = fetch, pause = ms => new Promise(resolve => setTimeout(resolve, ms)), attempts = 60 } = {}) {
  for (let attempt = 0; attempt < attempts; attempt++) {
    const visible = await Promise.all(manifests.map(m => releaseVisible(m, request)));
    if (visible.every(Boolean)) return;
    if (attempt === 0 || (attempt + 1) % 6 === 0) console.log(`等待 npm 公开版本：${manifests.filter((_, i) => !visible[i]).map(m => m.name).join(", ")}`);
    if (attempt + 1 < attempts) await pause(5000);
  }
  throw new Error("npm 仍在处理发布，请等待版本公开后重跑相同工作流");
}

export async function verifyReadme(name, expected) {
  for (let attempt = 0; attempt < 60; attempt++) {
    const response = await fetch(`https://registry.npmjs.org/${encodeURIComponent(name)}`, { headers: { "cache-control": "no-cache" }, signal: AbortSignal.timeout(15000) });
    if (!response.ok) throw new Error(`读取 npm README 失败：HTTP ${response.status}`);
    const value = await response.json();
    if (value.readme?.trim() === expected.trim()) {
      console.log("npm README 与仓库 README 一致");
      return;
    }
    if (attempt === 0 || (attempt + 1) % 6 === 0) console.log("等待 npm README 更新");
    await new Promise(resolve => setTimeout(resolve, 5000));
  }
  throw new Error("npm README 与仓库内容不一致");
}
