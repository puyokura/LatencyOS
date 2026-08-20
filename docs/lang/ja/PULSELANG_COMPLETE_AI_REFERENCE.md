# PulseLang v2 完全自律AI仕様書 & コード生成マニュアル (日本語版)

> **ドキュメント種別**: 単一ファイル完結型 AI インコンテキスト学習 & 自動コード生成リファレンス
> **対象読者**: AIコーディングエージェント、LLM、コンパイラ開発者、形式検証エンジン
> **言語バージョン**: `2.0.0-hard-realtime`
> **実行環境**: `LatencyOS (x86_64 freestanding no_std)`
> **自己完結保証**: 本ファイル 1 つを LLM / AI エージェントのコンテキストに投入するだけで、構文・型システム・線形型所有権・全組み込み関数・ISA・生成テンプレート・アンチパターンを 100% 網羅し、誤りのない PulseLang v2 コードを生成できます。

---

## 1. AI システムプロンプト & 5大不変条件 (Core Invariants)

あなたは **LatencyOS** 上で動作する **PulseLang v2** の専門コンパイラ兼自律コード生成エージェントです。

PulseLang のコードを生成する際は、**必ず以下の 5 つの不変条件を例外なく厳守**してください：

1. **接頭辞ディシプリン (曖昧性ゼロ原則)**:
   - **`$`**: すべての変数（例: `$rtt`, `$sum`, `$i`, `$t0`, `$dt`）。静的 32 レジスタスロット（`$0`〜`$31`）にバインド。
   - **`#`**: すべてのハードウェア/DMA 線形ハンドル（例: `#f`, `#packet`, `#frame`）。複製不能・単一所有権。
   - **`@`**: すべての契約、時間制御構文、組み込み関数（例: `@contract`, `@pipeline`, `@on_vblank`, `@within`, `@while`, `@tsc()`, `@rtt()`, `@rate()`, `@capture()`, `@send()`, `@print()`, `@println()`）。
2. **線形型（Linear Type）の単一消費保証**:
   - `#f := @capture();` で取得したハンドルは、**すべての実行分岐で厳密に 1 回だけ消費（通常は `@send(#f);`）** されなければならない。
   - 二重解放、解放漏れ、複製（Copy）はコンパイルエラーとなります。
3. **時間単位の必須付与**:
   - 時間定数には必ず明示的な単位接尾辞を付与する: `ns`（ナノ秒）, `us`（マイクロ秒）, `ms`（ミリ秒）, `s`（秒）。
   - コンパイル時に即座に 64 ビット整数のナノ秒値へ展開されます（例: `500us` $\to$ `500000`）。
4. **文末セミコロンの必須化**:
   - すべての文末には必ずセミコロン `;` を付与する。
5. **動的メモリ確保・非決定論的ループの禁止**:
   - ヒープ確保（`malloc`/`Box`）、動的再帰、無限ループは存在しません。
   - すべてのループは有界（Bounded）であり、最大 10,000 ステップで強制終了されます。

---

## 2. 完全形式文法 (EBNF)

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

## 3. ハードウェア組み込み関数カタログ

| 関数名 | シグネチャ | 最悪実行時間 | 説明 |
|---|---|---|---|
| `@tsc()` | `() -> i64` | **~15 ns** | ハードウェア TSC（`rdtscp`）のシリアル化現在値を返す |
| `@rtt()` | `() -> i64` | **~20 ns** | PMD ドライバが計測した最新の RTT（ナノ秒）を取得 |
| `@rate(pct)` | `(i64) -> ()` | **~10 ns** | 送信レート（10〜100%）を設定 |
| `@capture()` | `() -> #handle` | **~100 ns** | GPU フレームバッファの線形記述子スロットを取得 |
| `@send(#h)` | `(#handle) -> ()` | **~200 ns** | NIC 送信リングへゼロコピー送出し、所有権を移動 |
| `@print(v)` | `(Any) -> ()` | **~500 ns** | シリアル出力（改行なし） |
| `@println(v)` | `(Any) -> ()` | **~500 ns** | シリアル出力（改行あり、CRLF 自動正規化） |

---

## 4. バイトコード ISA 仕様

| Opcode | ニーモニック | オペランド | スタック効果 | 説明 |
|---|---|---|---|---|
| `0x01` | `OP_PUSH_CONST` | `i64` (8B) | `[] -> [val]` | 即値をプッシュ |
| `0x02` | `OP_LOAD_VAR` | `u8` (1B) | `[] -> [var[idx]]` | レジスタスロットからロード |
| `0x03` | `OP_STORE_VAR` | `u8` (1B) | `[val] -> []` | スタック先頭をレジスタへ保存 |
| `0x04` | `OP_ADD` | なし | `[a, b] -> [a + b]` | 加算 |
| `0x05` | `OP_SUB` | なし | `[a, b] -> [a - b]` | 減算 |
| `0x06` | `OP_MUL` | なし | `[a, b] -> [a * b]` | 乗算 |
| `0x07` | `OP_DIV` | なし | `[a, b] -> [a / b]` | 除算（0除算保護） |
| `0x08` | `OP_MOD` | なし | `[a, b] -> [a % b]` | 剰余 |
| `0x09` | `OP_CMP_EQ` | なし | `[a, b] -> [a == b]` | 一致比較 |
| `0x0A` | `OP_CMP_NE` | なし | `[a, b] -> [a != b]` | 不一致比較 |
| `0x0B` | `OP_CMP_LT` | なし | `[a, b] -> [a < b]` | 小なり比較 |
| `0x0C` | `OP_CMP_LE` | なし | `[a, b] -> [a <= b]` | 以下比較 |
| `0x0D` | `OP_CMP_GT` | なし | `[a, b] -> [a > b]` | 大なり比較 |
| `0x0E` | `OP_CMP_GE` | なし | `[a, b] -> [a >= b]` | 以上比較 |
| `0x0F` | `OP_JUMP` | `u16` (2B) | `[] -> []` | 無条件ジャンプ |
| `0x10` | `OP_JUMP_IF_FALSE`| `u16` (2B) | `[cond] -> []` | 偽（0）ならジャンプ |
| `0x11` | `OP_CALL_NATIVE` | `u8, u8` | `[args...] -> [res]` | ハードウェア組み込み命令呼出 |
| `0x12` | `OP_WITHIN_START`| `i64` (8B) | `[] -> []` | デッドラインタイマー開始 |
| `0x13` | `OP_WITHIN_END` | なし | `[] -> []` | デッドライン検証 |
| `0x14` | `OP_DROP` | なし | `[] -> []` | 超過フレーム破棄 |
| `0x15` | `OP_PUSH_STR` | `u16, u16` | `[] -> [ptr]` | 文字列プール参照をプッシュ |
| `0x16` | `OP_HALT` | なし | `[] -> []` | スクリプト実行終了 |

---

## 5. 実践コード生成テンプレート 10 選

### テンプレート 1: ゼロコピー GPU-to-NIC パイプライン (`stream.pl`)
```pulse
// stream.pl - Zero-Copy Ultra-Low-Latency Pipeline
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

### テンプレート 2: 有界反復数学ベンチマーク (`bench.pl`)
```pulse
// bench.pl - Real-Time Bounded Iteration Benchmark
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

### テンプレート 3: 適応型輻輳制御ガード (`filter.pl`)
```pulse
// filter.pl - Adaptive Congestion Controller
@contract: @wcet(2us) @budget(100us);

$rtt := @rtt();
@println("[FILTER] Measured RTT (ns):");
@println($rtt);

$rtt > 300us ? {
    @println("[ACTION] Congestion detected -> Rate: 60%");
    @rate(60);
} : {
    @println("[ACTION] Optimal latency -> Rate: 100%");
    @rate(100);
};
```

### テンプレート 4: ハードウェアジッタ解析 (`jitter.pl`)
```pulse
// jitter.pl - Cycle-Accurate Jitter Analyzer
@contract: @wcet(3us) @budget(30us);

$t1 := @tsc();
$t2 := @tsc();
$delta := $t2 - $t1;

@println("[JITTER] Consecutive TSC Delta (Cycles):");
@println($delta);

$delta < 100 ? {
    @println("[STATUS] Determinism: Optimal (<100 cycles)");
} : {
    @println("[STATUS] Determinism: Jitter detected");
};
```

### テンプレート 5: リアルタイムハードウェアテレメトリ (`telemetry.pl`)
```pulse
// telemetry.pl - Real-Time Hardware Telemetry Inspector
@contract: @wcet(2us) @budget(20us);

$rtt := @rtt();
$tsc := @tsc();

@println("=== LatencyOS Hardware Telemetry ===");
@println("[CLOCK] Serialized TSC Ticks:");
@println($tsc);
@println("[NET] Active Round-Trip Time (ns):");
@println($rtt);

$rtt < 100us ? @println("[HEALTH] Sub-100us glass-to-glass latency guaranteed.") : @println("[HEALTH] RTT backpressure active.");
```

---

## 6. AI が犯しやすいアンチパターンと修正例

| 誤ったコード | 失敗原因 | 正しいコード |
|---|---|---|
| `let x = 10;` | `let` / `var` キーワードは存在しない | `$x := 10;` |
| `f := @capture();` | ハードウェア DMA ハンドルには `#` が必須 | `#f := @capture();` |
| `while ($i < 10) {}` | 制御構文には `@` が必須 | `@while($i < 10) {}` |
| `delay(10);` | スリープ関数は存在しない | `@within(Time) {}` を使用 |
| `malloc(1024);` | 動的ヒープメモリは完全禁止 | 静的 `$var` スロットを使用 |
| `print("hi")` | 組み込み関数には `@` が必須 | `@print("hi");` |
| `@send(#f)` の欠落 | 線形ハンドル `#f` の解放漏れ | 全分岐で `@send(#f)` を呼ぶ |
| `@within(500)` (単位なし) | 時間定数には単位接尾辞が必須 | `@within(500us)` |
| `$sum = $sum + 1` (`;`なし)| 文末セミコロンが必須 | `$sum += 1;` |

---

## 7. AI コード生成前チェックリスト

コードを出力する前に、以下の全項目をチェックしてください：

- [ ] すべての変数が `$` で始まっているか（`$var`）。
- [ ] すべてのハードウェアハンドルが `#` で始まっているか（`#handle`）。
- [ ] すべてのディレクティブ・組み込み命令が `@` で始まっているか（`@contract`, `@tsc()` 等）。
- [ ] すべての文末に `;` が付いているか。
- [ ] すべての `#handle` が全分岐で厳密に 1 回消費されているか。
- [ ] すべての時間定数に単位（`ns`, `us`, `ms`, `s`）が付いているか。
- [ ] すべての `@while` ループに変数の単調増減（`$i += 1;` 等）があるか。
- [ ] `let`, `var`, `function`, `def`, `class`, `malloc`, `free`, `return` などの汎用言語キーワードを使っていないか。
