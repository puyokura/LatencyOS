# PulseLang v2 言語ドキュメントポータル (日本語)

PulseLang v2 は、LatencyOS カーネルに直接統合された **AIネイティブ・時間駆動型リアクティブ DSL（Domain Specific Language）** です。

---

## 日本語ドキュメント構成

| ドキュメント | 概要 | 対象読者 |
|---|---|---|
| [**形式言語仕様書 (`spec.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/spec.md) | 言語仕様、形式 EBNF 文法、型システム、契約仕様 | 開発者、アーキテクト |
| [**AI向け形式仕様書 (`ai_spec.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/ai_spec.md) | 機械可読文法、AI生成不変条件、標準テンプレート | AIエージェント、LLM |
| [**バイトコード ISA 仕様書 (`isa.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/isa.md) | VMアーキテクチャ、スタックマシン、Opcode一覧 | コンパイラ・VM開発者 |
| [**スクリプトクックブック (`cookbook.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/cookbook.md) | 標準スクリプト解説とリアルタイム実践レシピ集 | アプリケーション開発者 |

---

## 主な特徴

1. **第一級市民としての時間リテラル**: `50ns`, `200us`, `5ms`, `1s` をコンパイル時にナノ秒へ即値展開。
2. **線形型（Linear Type）によるハードウェア管理**: GPU/NIC 記述子（`#f`）の単一所有権と二重解放防止。
3. **明示的契約ディレクティブ**: `@contract: @wcet(5us) @budget(50us);` による静的・動的遅延保証。
4. **ゼロヒープ・単一パスコンパイル**: ヒープアロケーションなしで決定論的にコンパイル（< 50 \textmu s）。
