# PulseLang v2 完全自律AI仕様書 & システムアーキテクチャ定義書 (日本語版)

> **ドキュメント種別**: 単一ファイル完結型 AI 形式仕様書・意味論 & 低レイヤアーキテクチャ定義書
> **対象読者**: AIコーディングエージェント、LLM、静的解析器、形式検証エンジン、カーネルエンジニア
> **言語バージョン**: `2.0.0-hard-realtime`
> **実行環境**: `LatencyOS (x86_64 freestanding no_std)`
> **自己完結保証**: 本ドキュメント 1 つで、構文・型システム・線形論理・全組み込み命令・バイトコード ISA・メモリ順序付け・DMA コヒーレンシ・43項目の形式アーキテクチャ契約を 100% 網羅しています。

---

## 1. AI システムプロンプト & 5大不変条件 (Invariants)

AI が PulseLang v2 のコードを生成する際は、**必ず以下の 5 つの不変条件を厳守** してください：

1. **接頭辞規則 (曖昧性ゼロ)**:
   - **`$`**: 変数（`$rtt`, `$sum`, `$i`, `$t0`, `$dt`）。静的 32 スロット（`$0`〜`$31`）にバインド。
   - **`#`**: ハードウェア/DMA 線形ハンドル（`#f`, `#packet`, `#frame`）。複製不能・単一消費。
   - **`@`**: 契約・時間制御・組み込み命令（`@contract`, `@pipeline`, `@on_vblank`, `@within`, `@while`, `@tsc()`, `@rtt()`, `@rate()`, `@capture()`, `@send()`, `@print()`, `@println()`）。
2. **線形型（Linear Type）の単一消費保証**:
   - `#f := @capture();` で取得したハンドルは、**すべての実行パスで厳密に 1 回だけ消費（通常は `@send(#f);`）** すること。
3. **時間単位の必須付与**:
   - 時間定数には必ず単位接尾辞を付与する（`ns`, `us`, `ms`, `s`）。コンパイル時に即座に整数ナノ秒へ展開。
4. **文末セミコロンの必須化**:
   - すべての文末には必ずセミコロン `;` を付与する。
5. **動的メモリ確保・非決定論的ループの禁止**:
   - ヒープ確保（`malloc`/`Box`）、動的再帰、無限ループは存在しない。最大 10,000 命令で強制停止。

---

## 2. 形式文法 (EBNF)

```ebnf
Script          ::= TopLevelDecl* <EOF>

TopLevelDecl    ::= ContractDecl
                  | PipelineDecl
                  | OnVblankDecl
                  | Statement

ContractDecl    ::= "@contract:" ("@wcet(" TimeLiteral ")")? ("@budget(" TimeLiteral ")")? ";"
PipelineDecl    ::= "@pipeline:" Identifier ("@budget(" TimeLiteral ")")? (";" | Block)
OnVblankDecl    ::= "@on_vblank:" Block ";"?

Statement       ::= AssignStmt
                  | CompoundAssign
                  | WithinStmt
                  | WhileStmt
                  | IfStmt
                  | ExprStmt
                  | Block

AssignStmt      ::= (VarIdent | HardwareIdent) ":=" Expression ";"
CompoundAssign  ::= (VarIdent | HardwareIdent) ( "+=" | "-=" ) Expression ";"
WithinStmt      ::= "@within(" TimeLiteral ")" Block ("!drop")? ";"
WhileStmt       ::= "@while(" Expression ")" Block
IfStmt          ::= "if" "(" Expression ")" Block ( "else" Block )?
ExprStmt        ::= Expression ";"

Block           ::= "{" Statement* "}"

Expression      ::= PipeExpr
PipeExpr        ::= TernaryExpr ( "|>" TernaryExpr )*
TernaryExpr     ::= LogicOrExpr ( "?" ( Block | Expression ) ":" ( Block | Expression ) )?
LogicOrExpr     ::= LogicAndExpr ( "||" LogicAndExpr )*
LogicAndExpr    ::= EqualityExpr ( "&&" EqualityExpr )*
EqualityExpr    ::= RelationalExpr ( ( "==" | "!=" ) RelationalExpr )*
RelationalExpr  ::= AdditiveExpr ( ( "<" | "<=" | ">" | ">=" ) AdditiveExpr )*
AdditiveExpr    ::= Multiplicative ( ( "+" | "-" ) Multiplicative )*
Multiplicative  ::= UnaryExpr ( ( "*" | "/" | "%" ) UnaryExpr )*
UnaryExpr       ::= ( "!" | "-" )? PrimaryExpr

PrimaryExpr     ::= IntegerLiteral
                  | TimeLiteral
                  | StringLiteral
                  | VarIdent
                  | HardwareIdent
                  | IntrinsicCall
                  | "(" Expression ")"

IntrinsicCall   ::= ( "@tsc" | "@rtt" | "@rate" | "@capture" | "@send" | "@print" | "@println" ) "(" ArgList? ")"
ArgList         ::= Expression ( "," Expression )*

IntegerLiteral  ::= [0-9]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 3. 43項目マスターアーキテクチャ & 意味論定義

### 1. 仕様間 WCET 値の完全統一
すべてのドキュメント・コンパイラ・シェル計測において以下の最悪実行時間を統一：
- バイトコード基本命令ディスパッチ: **25 ns**
- `@tsc()`: **15 ns**
- `@rtt()`: **20 ns**
- `@rate()`: **10 ns**
- `@capture()`: **100 ns**
- `@send()`: **200 ns**
- `@print()` / `@println()`: **500 ns**
- パイプライン総遅延予算: **8,000 \textmu s (8.00 ms)**

### 2. 組み込み命令 WCET と VM 命令 WCET の関係
プログラム全体の静的 WCET は以下のように算出されます：
$$\text{WCET}_{\text{total}} = \sum (\text{Opcode 数} \times 25\text{ ns}) + \sum (\text{組み込み命令 WCET})$$

### 3. Time 型と i64 型の型規則
`Time` はコンパイル時にナノ秒単位の `u64` へ即値展開され、VM 内部では `i64` として扱われます。`Time` と `i64` の四則演算や比較は実行時キャストなしで直接評価されます。

### 4. String Tagged Pointer の意味論
静的 512 バイト文字列プール内の文字列は、VM スタック上でタグ付き 64 ビットポインタとして保持されます：
$$\text{Ptr} = \mathtt{0x7FFF\_0000\_0000\_0000} \mid (\text{len} \ll 16) \mid \text{offset}$$
VM はアクセス前に $\text{offset} + \text{len} \le 512$ の境界チェックを行います。

### 5. Handle と DMA 完了の関係
`#f := @capture()` は GPU キャプチャリングからスロット番号を確保し、`@send(#f)` は記述子を NIC 送信リングへ移譲して `sfence` を発行します。DMA 完了は記述子ステータス（`E1000_TXD_STAT_DD`）のポーリングで検知されます。

### 6. `!drop` 時の Handle ライフサイクル
`@within(Time) { ... } !drop;` の制限時間を超過した場合、`OP_DROP` が実行され、未送信の `#handle` 記述子は直ちに破棄・解放され、古いフレームの送信を防止します。

### 7. `OP_CALL_NATIVE` の ABI
- Opcode: `0x11`
- オペランド 1 (`u8`): `func_id` (`1`〜`7`)
- オペランド 2 (`u8`): `argc`（引数の個数）
引数は VM スタックから逆順（右から左）でポップされます。

### 8. `OP_CALL_NATIVE` の戻り値規則
- 戻り値なしの命令（`@rate`, `@println`, `@send`）: 何もプッシュしない、または `0` をプッシュ。
- 戻り値ありの命令（`@tsc`, `@rtt`, `@capture`）: 結果値（`i64` または `handle_id`）をスタックへプッシュ。

### 9. `OP_WITHIN_START` / `OP_WITHIN_END` のネスト規則
VM は 8 階層のデッドラインスタックを保持します。内側のデッドラインは外側のデッドライン以下の値でなければなりません（$\text{Deadline}_{\text{inner}} \le \text{Deadline}_{\text{outer}}$）。

### 10. `OP_DROP` の実行条件
`read_tsc() > deadline_tsc` である場合のみ `OP_DROP` (`0x14`) が実行されます。

### 11. VM 中断時のリソース回収
ステップ数上限（10,000命令）超過や実行時エラーで中断した場合：
1. デッドラインスタックポインタ（`dl_sp`）を 0 にリセット。
2. 32 個の変数スロットをクリア。
3. 消費されなかった `#handle` 記述子を回収。

### 12. 制御構文の統一
- `if (cond) { ... } else { ... }` は `OP_JUMP_IF_FALSE` と `OP_JUMP` にコンパイル。
- 三項演算子 `$cond ? expr1 : expr2` および三項ブロック `$cond ? { ... } : { ... };` も同一のジャンプ意味論を共有。
- `@contract`, `@within`, `@while` は時間・リアルタイム制約を付与。

### 13. 分岐における Handle の型検査
分岐前に取得された `#handle` は、`if/else` や三項演算子の**両方の分岐で消費**されなければコンパイルエラーとなります。

### 14. ループ内での Handle 使用規則
`@while` ループ内で取得された `#handle` は、同一反復内で消費されなければなりません。ループ境界をまたぐことはできません。

### 15. `@capture` 失敗時の状態
GPU フレームリングが枯渇している場合、`@capture()` は `0`（null 記述子）を返します。

### 16. `@send` 失敗時の状態
NIC 送信リングが満杯の場合、`@send()` はフレームをドロップしてバックプレッシャカウンタを加算し、ハンドルは消費済みとして扱われます。

### 17. ゼロ除算の正確な意味論
`OP_DIV` (`0x07`) および `OP_MOD` (`0x08`) で除数が `0` の場合、CPU トラップを起こさず `0` を返します。

### 18. 整数オーバーフローの意味論
すべての 64 ビット整数演算は 2 の補数でラップアラウンド（`wrapping_add`, `wrapping_sub`, `wrapping_mul`）します。

### 19. 比較演算のブール表現
比較演算（`OP_CMP_EQ` 〜 `OP_CMP_GE`）は真のとき `1`、偽のとき `0` をプッシュします。

### 20. ブールの内部型
ブール値は `i64` で表されます（`0` は偽、非ゼロは真）。

### 21. 文字列ポインタのメモリ安全性
文字列ポインタは固定の読み取り専用プール内のオフセットで管理され、領域外アクセスは VM によって拒否されます。

### 22. 静的文字列プールの上限
文字列リテラルの合計が 512 バイトを超えるとコンパイルエラーとなります。

### 23. VM スタックオーバーフロー保護
VM スタックは最大 64 要素で固定され、超過時は `Err("Stack overflow")` を返します。

### 24. バイトコード検証とフォーマット
実行前にマジック `PX64`（`0x50583634`）または `PULS`（`0x50554C53`）、コード長、16 バイト固定ヘッダー、4 バイト命令アライメントの妥当性を検証します。

### 25. バイトコードバージョン
ヘッダーのバイト 4-5 にバージョン `2`（`0x0002`）が指定されている必要があります。

### 26. 組み込み命令 ID の ABI 定義
- `1`: `NATIVE_PRINT` (`@print`)
- `2`: `NATIVE_PRINTLN` (`@println`)
- `3`: `NATIVE_SYS_TSC` (`@tsc`)
- `4`: `NATIVE_NET_RTT` (`@rtt`)
- `5`: `NATIVE_NET_SET_RATE` (`@rate`)
- `6`: `NATIVE_GPU_CAPTURE` (`@capture`)
- `7`: `NATIVE_NET_SEND` (`@send`)
- `8`: `NATIVE_SCRIPT_ARGC` (`@argc`)
- `9`: `NATIVE_SCRIPT_ARG` (`@arg`)


### 27. ハードウェアターゲット仕様
- CPU: x86_64（Invariant TSC 対応）
- NIC: Intel 82540EM / 82545EM (e1000) PMD
- GPU: Linear Framebuffer 1920x1080 @ 32bpp
- コア構成: 4 コア SMP（役割固定）

### 28. TSC の時間単位
1 TSC tick = 1 CPU クロックサイクル（3.40 GHz の場合 約 0.294 ns）。

### 29. TSC ticks とナノ秒の変換式
$$\text{Nanoseconds} = \frac{\text{Ticks} \times 1,000,000,000}{\text{TSC Frequency (Hz)}}$$

### 30. CPU 周波数変動と C-State 固定
全 4 コアは MSR `0x1A0`（`MISC_ENABLE`）および MSR `0x1B0`（`ENERGY_PERF_BIAS = 0x0`）で C0 ステートに固定され、クロック周波数変動によるジッタを排除します。

### 31. 割り込みによる WCET 変動
- Core 1〜3 は割り込み禁止（`cli`）で動作。
- Core 0 のみタイマーおよびシリアル割り込みを処理（ISR 実行時間は $\le 150\text{ ns}$ に制限）。

### 32. キャッシュミスを考慮した WCET モデル
最悪実行時間モデルは、ホットループの L1/L2 キャッシュ保持（< 4 ns）と、コールド DRAM アクセス上限（100 ns）を考慮して算出されます。

### 33. DMA キャッシュコヒーレンシ
DMA メモリ領域は非キャッシュ（UC）またはライトコンバイニング（WC）に設定され、`sfence` / `clflush` により CPU-NIC 間の整合性を保証します。

### 34. `sfence` / `mfence` の発行条件
- フレーム記述子の書き込み後に `sfence` を発行。
- SPSC リングバッファのテールポインタ更新時に `mfence` を発行。

### 35. 4 コア間のメモリ順序付け
コア間通信は SPSC Lock-Free キューを用い、x86 TSO に整合する atomic `Acquire`/`Release` 順序付けを適用。

### 36. VBLANK イベントの競合排除
Core 1 のみが GPU VBLANK レジスタをポーリングし、SMP ロック競合を完全排除。

### 37. パイプラインバッファのライフサイクル
`Stage 0 (ISR)` $\to$ `Stage 1 (Userspace)` $\to$ `Stage 2 (VBLANK)` $\to$ `Stage 3 (Capture)` $\to$ `Stage 4 (Encode)` $\to$ `Stage 5 (Network TX)` $\to$ `Ring Release`。

### 38. DMA バッファのライフサイクル
`スロット解放` $\to$ `キャプチャ割当` $\to$ `DMA 転送` $\to$ `TX 完了` $\to$ `空きプールへ回収`。

### 39. NIC TX 完了ポーリング
Core 3 が e1000 TX 記述子のステータスビット `E1000_TXD_STAT_DD` を割り込みなしでポーリング。

### 40. GPU バッファの完了検知
次フレームの VBLANK エッジ検知時に前フレームのスロットをリサイクル。

### 41. コンパイラエラーリカバリ
単一パスコンパイラが構文エラーの発生行・列・トークンを含む構造化 `Result` を即時返却。

### 42. 有界ループ証明 (Bounded Loop Proof) の形式仕様
`@while(cond)` は条件変数の単調増減（`$i += 1;` 等）を要求し、進捗のないループは VM の 10,000 命令制限で強制停止。

### 43. 静的 WCET と動的 TSC 実測値の不一致時の扱い
静的 WCET は理論上の安全上限値を提供し、実行時はハードウェア TSC で動的遅延を計測。`@within` を超過した場合は `!drop` が即座に発動して破棄。

---

## 4. 標準スクリプト一覧

### 4.1 ゼロコピー GPU-to-NIC パイプライン (`stream.pl`)
```pulse
// stream.pl - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
@pipeline: UltraStream @budget(8000us);

@on_vblank: {
    #f := @capture();
    @within(500us) {
        $rtt := @rtt();
        $rtt > 200us ? @rate(80) : @rate(100);
        @send(#f);
    } !drop;
};
```

### 4.2 レイテンシベンチマーク (`bench.pl`)
```pulse
// bench.pl - Realtime Math & Latency Benchmark
@contract: @wcet(5us) @budget(50us);

$t0 := @tsc();
$sum := 0;
$i := 0;

@while($i < 100) {
    $sum += $i * 2;
    $i += 1;
}

$dt := @tsc() - $t0;
@println("[BENCH] Iterations: 100");
@println("[RESULT] Sum:");
@println($sum);
@println("[LATENCY] Cycles:");
@println($dt);
```

---

## 5. モジュールシステム & リアルタイム合成アーキテクチャ

PulseLang v2 は、動的割り当てゼロで静的に形式検証されるハードリアルタイムモジュールシステムを導入しています。

### 5.1 モジュール宣言 & 構文
```pulse
// モジュール宣言 (ファイル先頭)
module net::congestion;

// インポート構文
import std::time;
import hw::e1000::{send, rate};

// 名前空間スコープ
namespace filter {
    pub $min_rtt := 50us;
    
    pub @func check_congestion($current_rtt) {
        $current_rtt > $min_rtt ? rate(80) : rate(100);
    };
}
```

### 5.2 公開/非公開シンボル
- **`pub $var` / `pub #handle`**: インポート元モジュールに公開されるシンボル。
- **`$var` / `#handle`**: 宣言元モジュール内に限定される非公開シンボル。
- **`pub @func(...)`**: 明示的な WCET 契約を持つ公開リアルタイム関数。

### 5.3 モジュール間の型参照 & ハンドル所有権移譲
- **線形型所有権の移譲**: インポート先関数へ `#handle` を渡すと所有権が移動し、呼び出し元での再参照はコンパイルエラーとなります。
- **型不変性**: 時間リテラルやレジスタ型はモジュール境界を越えて静的検証されます。

### 5.4 モジュール間の WCET 伝播
モジュール $A$ がモジュール $B$ の関数 $F$ を呼び出す場合：
$$\text{WCET}(A) = \text{WCET}_{\text{local}}(A) + \text{WCET}(B::F) + \text{呼び出しオーバーヘッド}(25\text{ ns})$$
コンパイラはインポートされた全モジュールについて $\text{WCET}(A) \le \text{Budget}(A)$ を静的検証します。

### 5.5 静的メモリ配置 & 名前空間分割
グローバル 32 スロット（`$0`〜`$31`）はコンパイル/リンク時に名前空間ごとに固定分割され、DRAM へのスタック退避や動的レジスタ割り当てを完全排除します。

### 5.6 循環依存の検出 & コンパイル順序
- **DAG 検査**: 循環インポート（`A -> B -> A`）はコンパイル時に検知され、`CircularDependencyError` として拒否されます。
- **トポロジカルソート**: モジュールは逆トポロジカル順序でコンパイルされ、独立した `.bin` バイトコード成果物を生成します。

### 5.7 標準ライブラリ & ハードウェア組み込みモジュール
- **`std::math`**: 固定小数点演算、クランプ、最小・最大値、線形補間。
- **`std::time`**: TSC サイクル変換、デッドライン比較、ジッター計測タイマー。
- **`std::net`**: RTT 推定器、輻輳ウィンドウ計算器、SRTP パケット整形。
- **`std::gpu`**: フレームバッファ記述子管理、VBLANK 同期。
- **`hw::e1000`**: Intel 82540EM PMD レジスタ直叩き。
- **`hw::tsc`**: 不変 CPU タイムスタンプカウンタ組み込み命令。
- **`hw::apic`**: コア間割り込み（IPI）および SMP 同期プリミティブ。

