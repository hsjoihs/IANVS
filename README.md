# IANVS
訪問者が Wi-Fi に接続するのを家主の代わりに監視する扉の守護神、ヤーヌス

## 設定 (Configuration)

環境変数で設定します。

### 必須の環境変数

- `DISCORD_TOKEN`: Discord ボットトークン

### Discord チャンネル設定

以下のいずれかの方法で通知先チャンネルを設定できます:

#### オプション1: 単一チャンネル (すべての通知を同じチャンネルに送信)
- `DISCORD_CHANNEL_ID`: Discord チャンネル ID (すべての通知用)

#### オプション2: 分離チャンネル (通知タイプごとに異なるチャンネル)
- `DISCORD_USER_NOTIFICATION_CHANNEL_ID`: ユーザーの入退出通知用チャンネル ID
- `DISCORD_MAC_INQUIRY_CHANNEL_ID`: 未連携MACアドレスの問い合わせ用チャンネル ID

**注意**: `DISCORD_CHANNEL_ID` を設定した場合、それが両方のメッセージタイプのデフォルトとして使用されます。個別に設定した場合はそちらが優先されます。

### その他の環境変数 (オプション)

- `ASSOCIATIONS_FILE`: MAC アドレスとユーザーの関連付けを保存する JSON ファイル (デフォルト: `associations.json`)
- `SCAN_INTERVAL_SECS`: ネットワークスキャンの間隔（秒） (デフォルト: 600)
- `PERSISTENCE_INTERVAL_SECS`: 状態保存の間隔（秒） (デフォルト: 60)
- `DISCORD_CHANNEL_CAPACITY`: Discord メッセージキューのサイズ (デフォルト: 100)

## Configuration (English)

Configure via environment variables.

### Required Environment Variables

- `DISCORD_TOKEN`: Discord bot token

### Discord Channel Configuration

You can configure notification destination channels in one of the following ways:

#### Option 1: Single Channel (send all notifications to the same channel)
- `DISCORD_CHANNEL_ID`: Discord channel ID (for all notifications)

#### Option 2: Separate Channels (different channels for different notification types)
- `DISCORD_USER_NOTIFICATION_CHANNEL_ID`: Channel ID for user entry/exit notifications
- `DISCORD_MAC_INQUIRY_CHANNEL_ID`: Channel ID for unlinked MAC address inquiries

**Note**: If `DISCORD_CHANNEL_ID` is set, it will be used as the default for both message types. Individual settings take precedence when specified.

### Other Environment Variables (Optional)

- `ASSOCIATIONS_FILE`: JSON file to store MAC address to user associations (default: `associations.json`)
- `SCAN_INTERVAL_SECS`: Network scanning interval in seconds (default: 600)
- `PERSISTENCE_INTERVAL_SECS`: State persistence interval in seconds (default: 60)
- `DISCORD_CHANNEL_CAPACITY`: Discord message queue size (default: 100)
