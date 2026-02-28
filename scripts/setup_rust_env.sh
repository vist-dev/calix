#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" = "0" ]; then
  echo "このスクリプトをrootで実行しないでください。通常ユーザーで実行してください。" >&2
  exit 1
fi

command_exists() { command -v "$1" >/dev/null 2>&1; }

echo "=== Rust 開発環境セットアップ ==="

# 必要なパッケージをインストール（Debian/Ubuntu の場合）
if [ -f /etc/debian_version ]; then
  echo "検出: Debian系。必要パッケージをインストールします（sudoパスワードが求められます）。"
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends curl build-essential pkg-config libssl-dev clang cmake
fi

# rustup のインストール
if ! command_exists rustup; then
  echo "rustup が見つかりません。rustup をインストールします。"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
else
  echo "rustup は既にインストールされています。"
fi

# Cargo 環境を有効化
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
fi

echo "ツールチェーンを最新のstableに設定します。"
rustup update
rustup default stable

echo "必要なコンポーネントを追加します: rustfmt, clippy, rust-src"
rustup component add rustfmt clippy rust-src || true

echo "cargo の便利ツールをインストールします（既にあればスキップされます）。"
declare -A tools=(
  [cargo-add]=cargo-edit
  [cargo-watch]=cargo-watch
  [cargo-make]=cargo-make
  [cargo-audit]=cargo-audit
  [cargo-outdated]=cargo-outdated
  [cargo-expand]=cargo-expand
)

for bin in "${!tools[@]}"; do
  pkg=${tools[$bin]}
  if command_exists "$bin"; then
    echo "$bin は既に存在します。"
  else
    echo "$pkg をインストールします（cargo install $pkg）。"
    cargo install --locked "$pkg" || true
  fi
done

# rust-analyzer を ~/.local/bin にインストール
RA_DIR="$HOME/.local/bin"
mkdir -p "$RA_DIR"
RA_BIN="$RA_DIR/rust-analyzer"
if command_exists rust-analyzer; then
  echo "rust-analyzer は既にインストールされています。"
else
  echo "rust-analyzer をダウンロードしてインストールします。"
  OS_TYPE=$(uname -s)
  ARCH_TYPE=$(uname -m)
  DOWNLOAD_URL=""
  if [ "$OS_TYPE" = "Linux" ]; then
    if [ "$ARCH_TYPE" = "aarch64" ] || [ "$ARCH_TYPE" = "arm64" ]; then
      DOWNLOAD_URL="https://github.com/rust-lang/rust-analyzer/releases/latest/download/rust-analyzer-linux-aarch64"
    else
      DOWNLOAD_URL="https://github.com/rust-lang/rust-analyzer/releases/latest/download/rust-analyzer-linux"
    fi
  elif [ "$OS_TYPE" = "Darwin" ]; then
    DOWNLOAD_URL="https://github.com/rust-lang/rust-analyzer/releases/latest/download/rust-analyzer-mac"
  fi

  if [ -n "$DOWNLOAD_URL" ]; then
    curl -L "$DOWNLOAD_URL" -o "$RA_BIN"
    chmod +x "$RA_BIN"
    echo "rust-analyzer を $RA_BIN にインストールしました。"
  else
    echo "rust-analyzer の自動インストールがサポートされていない環境です。手動でインストールしてください。"
  fi
fi

echo "パスに ~/.local/bin を追加していない場合は、シェル設定に追加してください。"
echo "例: export PATH=\"\$HOME/.local/bin:\$PATH\""

echo "セットアップ完了。シェルを再起動するか次を実行してください:"
echo "  source \"\$HOME/.cargo/env\""

echo "=== 完了 ==="
