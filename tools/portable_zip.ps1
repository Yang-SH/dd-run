# 便携包（portable zip）构建——**单一来源**。
#
# 背景（2026-10-17 收口批）：zip 此前只存在于 release.yml 的 pwsh 内联步骤里，
# 本地 `bash tools/package.sh` 只产出 exe + sidecar，导致「本地重打包」与
# 「Release 附件」成员集合不一致，每次都要手工补 zip。本脚本把该逻辑抽为
# 单一来源，package.sh（Windows 分支）与 release.yml 都调它。
#
# 布局（解压即用）：
#   dd-run-<ver>.exe
#   extensions.d/<清单>.json
#   extensions.d/dd-ext-search.exe
#   extensions.d/<清单>.json.sig   ← 若随包签名已生成（package.sh §6.5）
#
# ⚠️ 条目名一律**正斜杠**：`Compress-Archive` 写反斜杠会被解压工具当成文件名
# 字符（出现 `extensions.d\com.ddrun.filesearch.json` 这类条目），故显式用
# .NET `ZipArchive::CreateEntryFromFile`。
#
# 用法：
#   pwsh -NoProfile -File tools/portable_zip.ps1            # 版本取自 Cargo.toml
#   pwsh -NoProfile -File tools/portable_zip.ps1 -Version 0.2.1
#   pwsh -NoProfile -File tools/portable_zip.ps1 -Quiet      # 只产包不回显条目
# 退出码：0 成功；1 缺 exe 或缺 sidecar 目录等前置不满足。
[CmdletBinding()]
param(
  [string]$Version,
  [switch]$Quiet
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot   # 仓库根
Push-Location $root
try {
  if (-not $Version) {
    $m = Select-String -Path "crates/dd-gui/Cargo.toml" -Pattern '^version = "(.+)"'
    if (-not $m) { throw "无法从 crates/dd-gui/Cargo.toml 解析版本" }
    $Version = $m.Matches[0].Groups[1].Value
  }

  $exe = "dist/dd-run-$Version.exe"
  $sidecarDir = "dist/extensions.d"
  $zip = "dist/dd-run-$Version-portable.zip"

  if (-not (Test-Path $exe)) { throw "缺宿主单文件：$exe（先跑 tools/package.sh）" }
  if (-not (Test-Path $sidecarDir)) { throw "缺 sidecar 目录：$sidecarDir" }

  $members = @(Get-ChildItem $sidecarDir -File | Sort-Object Name)
  if ($members.Count -eq 0) { throw "$sidecarDir 为空（至少应有清单 + dd-ext-search.exe）" }

  if (Test-Path $zip) { Remove-Item $zip -Force }
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $archive = [System.IO.Compression.ZipFile]::Open($zip, [System.IO.Compression.ZipArchiveMode]::Create)
  try {
    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
      $archive, $exe, "dd-run-$Version.exe",
      [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
    foreach ($f in $members) {
      [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
        $archive, $f.FullName, "extensions.d/$($f.Name)",
        [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
  } finally {
    $archive.Dispose()
  }

  if ($Quiet) {
    Write-Host "portable zip: $zip"
  } else {
    $size = (Get-Item $zip).Length
    Write-Host "便携包已生成：$zip（$size B）"
    $z = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path $zip).Path)
    try {
      foreach ($e in $z.Entries) { Write-Host ("  {0}  {1} B" -f $e.FullName, $e.Length) }
      # 逆斜杠条目 = 解压错位隐患，直接失败
      $bad = @($z.Entries | Where-Object { $_.FullName.Contains('\') })
      if ($bad.Count) { throw "zip 含反斜杠条目名（$($bad.Count) 个）" }
    } finally {
      $z.Dispose()
    }
  }
} finally {
  Pop-Location
}
