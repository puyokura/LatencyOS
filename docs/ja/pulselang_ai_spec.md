# PulseLang v2 AI向け形式仕様書 & コード生成リファレンス (日本語)

> **対象読者**: AIコーディングエージェント、LLM、静的解析器、形式検証器、リアルタイムシステム開発者
> **言語バージョン**: `2.0.0-hard-realtime`
> **実行環境**: `LatencyOS (x86_64 freestanding no_std)`

---

## 1. AI コード生成における不変条件 (Invariants)

AI が PulseLang v2 のコードを生成する際、**必ず** 以下の構文制約を遵守してください：

1. **変数・ハンドル・ディレクティブの接頭辞規則**:
   - 変数: 必ず `$` で始める（例: `$rtt`, `$sum`, `$i`, `$t0`）
   - ハードウェア/DMA線形ハンドル: 必ず `#` で始める（例: `#f`, `#packet`）
   - 契約・制御構文・組み込み関数: 必ず `@` で始める（例: `@contract`, `@within`, `@while`, `@tsc()`）
2. **線形型（Linear Type）の厳格な単一消費**:
   - `#f := @capture();` で取得した `#handle` は、**すべての実行分岐で厳密に1回だけ消費（例: `@send(#f);`）** されなければならない。
   - 二重解放、解放漏れ、複製（Copy）はコンパイルエラーとなります。
3. **時間リテラルの単位指定**:
   - 時間定数は必ず単位接尾辞を付与する: `ns`（ナノ秒）, `us`（マイクロ秒）, `ms`（ミリ秒）, `s`（秒）。
   - コンパイル時に即座に 64 ビット整数のナノ秒値へ展開されます（例: `500us` $\to$ `500000`）。
4. **文末セミコロンの必須化**:
   - すべての文末には必ずセミコロン `;` を付与する。
5. **動的メモリ確保・非決定論的ループの禁止**:
   - ヒープ確保（`malloc`/`Box`）、動的再帰、無限ループは存在しません。
   - すべてのループは有界（Bounded）であり、最大 10,000 ステップで強制終了されます。

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

## 3. ハードウェア組み込み関数 (Intrinsics)

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

## 4. AI 生成用標準コードテンプレート

### テンプレート 1: ゼロコピー GPU-to-NIC パイプライン (`stream.pul`)
```pulse
// stream.pul - Zero-Copy Ultra-Low-Latency Pipeline
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

### テンプレート 2: 有界反復数学ベンチマーク (`bench.pul`)
```pulse
// bench.pul - Real-Time Bounded Iteration Benchmark
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

### テンプレート 3: 適応型輻輳制御ガード (`filter.pul`)
```pulse
// filter.pul - Adaptive Congestion Controller
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
