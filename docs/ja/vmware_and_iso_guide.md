# VMware / VirtualBox / 実機実行ガイド

LatencyOS は、Limine ブートローダプロトコルを採用し、**UEFI (x86_64)** および **Legacy BIOS** の双方に対応したハイブリッドブータブル ISO（`dist/LatencyOS.iso`）を自動生成できます。

---

## 1. ブータブル ISO の生成方法

ISO イメージは `cargo xtask` コマンドで簡単にビルド・生成できます：

```powershell
# カーネル、ホストコンパイラ、および dist/LatencyOS.iso をビルド
cargo xtask iso --release

# または全成果物 (LatencyOS.exe, pulc.exe, LatencyOS.iso) を一括生成
cargo xtask dist
```

生成される成果物：
- `dist/LatencyOS.iso`: ハイブリッドブータブル ISO（El Torito BIOS + UEFI System Partition）。

ISO が正常にブートするか検証する自動テスト：
```powershell
cargo xtask test-iso
```

---

## 2. VMware Workstation / Player での実行

### ステップ 1: 仮想マシンの新規作成
1. VMware Workstation または VMware Player を起動します。
2. **新規仮想マシンの作成**（標準 / 推奨）を選択します。
3. **インストーラ ディスク イメージ ファイル (iso)** で `dist/LatencyOS.iso` を指定します。
4. ゲスト OS の選択：
   - **ゲスト OS**: `その他 (Other)`
   - **バージョン**: `他の 64 ビット (Other 64-bit)`（または `他の Linux 5.x カーネル 64 ビット`）
5. 仮想マシン名を設定します（例: `LatencyOS`）。
6. ディスク容量：任意（1GB〜2GB程度。LatencyOS は全メモリ常駐型のため仮想ディスクなしでも動作します）。

### ステップ 2: ハードウェア構成のカスタマイズ
**ハードウェアをカスタマイズ** を開きます：
- **メモリ**: 最低 `128 MB`（推奨: `512 MB` または `1024 MB`）。
- **プロセッサ**: `4` コア（LatencyOS は 4 コアの静的リアルタイムパイプライン前提で動作します）。
  - `VT-x/AMD-V の仮想化` または CPU パフォーマンス カウンタの仮想化を有効に設定。
- **ネットワーク アダプタ**: NAT または ブリッジ。
  - LatencyOS は Intel 82540EM (e1000) 用のポーリングモードドライバを内蔵しています。
  - `.vmx` 構成ファイルで `ethernet0.virtualDev = "e1000"` となっていることを確認します。
- **ディスプレイ**: 標準 VGA。

### ステップ 3: 起動
1. 仮想マシンをパワーオンします。
2. Limine ブートローダのメニューが表示されます：
   - `LatencyOS (x86_64 ELF)`
3. Enter キーを押すか、3 秒待つと自動で起動します。
4. Core 0 が起動し、APIC 経由で Core 1〜3 をウェイクアップし、パイプラインを構築して Pulse Shell プロンプトを表示します：
   ```text
   LatencyOS 0.0.5 (x86_64 hard-realtime)
   [c0|18ns] %
   ```

---

## 3. Oracle VirtualBox での実行

### ステップ 1: 仮想マシンの作成
1. VirtualBox を起動し、**新規** をクリックします。
2. **名前**: `LatencyOS`
3. **タイプ**: `Other`
4. **バージョン**: `Other/Unknown (64-bit)`
5. **メインメモリ**: `512 MB` 以上
6. **プロセッサ**: `4` CPU

### ステップ 2: ストレージおよびネットワーク設定
1. **設定 -> ストレージ** を開きます。
2. 光学ドライブに `dist/LatencyOS.iso` を割り当てます。
3. **設定 -> ネットワーク -> アダプター 1**:
   - 割り当て: `NAT`（またはブリッジアダプター）
   - 高度 -> アダプタータイプ: `Intel PRO/1000 MT Desktop (82540EM)`

### ステップ 3: 起動
1. 仮想マシンを起動します。
2. Limine メニューから `LatencyOS` を選択して Enter を押すと、Pulse Shell が起動します。

---

## 4. 実機（ベアメタル）での USB 起動

`dist/LatencyOS.iso` は **isohybrid** 形式で出力されるため、USB メモリに raw ブロック書き込みすることでそのまま起動メディアになります：

### Windows（Rufus を使用）:
1. USB メモリを挿入します。
2. **Rufus** を起動し、ブートの種類で `LatencyOS.iso` を選択します。
3. 「スタート」押下時、**DD イメージモード** で書き込むを選択します。

### Linux / macOS (`dd` コマンド):
```bash
sudo dd if=dist/LatencyOS.iso of=/dev/sdX bs=4M status=progress conv=fsync
```
*(※ `/dev/sdX` はお使いの USB ドライブのブロックデバイス名)*

PC 起動時に F11 / F12 / Esc などのブートメニューから UEFI / BIOS 起動を選択してください。
※ CPU が x86_64, SSE4.2, AES-NI に対応している必要があります。
