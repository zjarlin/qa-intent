// 启动器、打包器和安装测试共用的平台清单。
module.exports = [
  { pkg: "qa-intent-darwin-arm64", os: "darwin", cpu: "arm64", rust: "aarch64-apple-darwin", exe: "qai" },
  { pkg: "qa-intent-darwin-x64", os: "darwin", cpu: "x64", rust: "x86_64-apple-darwin", exe: "qai" },
  { pkg: "qa-intent-linux-x64", os: "linux", cpu: "x64", rust: "x86_64-unknown-linux-gnu", exe: "qai" },
  { pkg: "qa-intent-linux-arm64", os: "linux", cpu: "arm64", rust: "aarch64-unknown-linux-gnu", exe: "qai" },
  { pkg: "@zjarlin/qa-intent-win32-x64", os: "win32", cpu: "x64", rust: "x86_64-pc-windows-msvc", exe: "qai.exe" },
];
