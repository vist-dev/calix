# Rust 開発環境セットアップ

リポジトリ付属のセットアップスクリプトで Rust 開発環境を素早く構築できます。

使い方:

1. スクリプトに実行権限を付与:

```bash
chmod +x scripts/setup_rust_env.sh
```

2. スクリプトを実行:

```bash
./scripts/setup_rust_env.sh
```

3. 実行後、現在のシェルに cargo 環境を反映:

```bash
source "$HOME/.cargo/env"
```

備考:
- Debian/Ubuntu 系で必要なシステムパッケージを `sudo` で自動インストールします。
- macOS などでは事前に `brew` 等で依存ツールを用意してください。
- `rust-analyzer` は自動で `~/.local/bin` に配置します。パスが通っていることを確認してください。
