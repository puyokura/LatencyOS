# PulseLang v2 形式言語仕様書 (日本語)

PulseLang は、LatencyOS 上で動作する **AIネイティブ・時間駆動型リアクティブ DSL（Domain Specific Language）** です。

---

## 1. 言語設計思想

1. **時間ファースト（Time-First）**:
   - 時間リテラル（`50ns`, `200us`, `5ms`, `1s`）が言語の第一級市民として組み込まれています。
2. **決定論的 WCET 保証**:
   - すべての処理はコンパイル時および実行時に最悪実行時間（WCET）予算と照合されます。
3. **ゼロヒープ・単一パスコンパイル**:
   - コンパイラは AST（抽象構文木）を構築せず、トークン列から直接バイトコードを生成（$O(N)$ 計算量）。
   - コンパイル自体がカーネル内部で < 50 \textmu s で決定論的に完了します。

---

## 2. 形式文法 (EBNF)

```ebnf
Script          ::= Statement* <EOF>
Statement       ::= ContractDecl
                  | PipelineDecl
                  | OnVblankDecl
                  | WithinStmt
                  | WhileStmt
                  | AssignStmt
                  | ExprStmt

ContractDecl    ::= "@contract:" ("@wcet(" TimeLiteral ")")? ("@budget(" TimeLiteral ")")? ";"
PipelineDecl    ::= "@pipeline:" Identifier ("@budget(" TimeLiteral ")")? (";" | Block)
OnVblankDecl    ::= "@on_vblank:" Block ";"?
WithinStmt      ::= "@within(" TimeLiteral ")" Block ("!drop")? ";"
WhileStmt       ::= "@while(" Expression ")" Block
AssignStmt      ::= (VarIdent | HardwareIdent) (":=" | "=" | "+=" | "-=") Expression ";"
ExprStmt        ::= Expression ";"

Block           ::= "{" Statement* "}"

Expression      ::= Ternary ( "|>" Ternary )*
Ternary         ::= Equality ( "?" ( Block | Expression ) ":" ( Block | Expression ) )?
Equality        ::= Comparison ( ( "==" | "!=" ) Comparison )*
Comparison      ::= Term ( ( "<" | "<=" | ">" | ">=" ) Term )*
Term            ::= Factor ( ( "+" | "-" ) Factor )*
Factor          ::= Primary ( ( "*" | "/" | "%" ) Primary )*
Primary         ::= Number
                  | TimeLiteral
                  | StringLiteral
                  | VarIdent
                  | HardwareIdent
                  | IntrinsicCall
                  | "(" Expression ")"

IntrinsicsCall   ::= ( "@print" | "@println" | "@tsc" | "@rtt" | "@rate" | "@capture" | "@send" | "@argc" | "@arg" ) "(" ArgList? ")"
ArgList         ::= Expression ( "," Expression )*

TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
```

---

## 3. 型システム

| 型名 | 内部表現 | 説明 | 例 |
|---|---|---|---|
| **`i64`** | 64-bit 符号付き整数 | 算術計算・汎用レジスタ | `42`, `-100` |
| **`TimeLiteral`** | 64-bit 符号なし整数 (ns) | コンパイル時に絶対ナノ秒値へ即値展開 | `500us` (→ 500,000) |
| **`LinearHandle`** | 8-bit スロット ID | 複製不能・単一所有権ハードウェア記述子 | `#f` |
| **`String` / `Arg`** | タグ付きポインタ (`0x4000_...` / `0x2000_...`) | 静的文字列プール / 引数バッファへのオフセット | `"Optimal"`, `@arg(0)` |

### 3.1 線形型（Linear Type）`#handle` の所有権規則
- `#f := @capture();` でハードウェアスロットが払い出されます。
- `@send(#f);` に渡された時点で所有権が NIC 送信リングへムーブ（移動）します。
- スコープを抜けるまでに消費されなかった場合、または二重解放（Double Free）が試みられた場合、コンパイラはエラーを出力します。

---

## 4. `@` 構文の 3 階層分類体系

| 分類 | 構文例 | 評価タイミング | 役割 |
|---|---|---|---|
| **Compiler Contracts** | `@contract:`, `@pipeline:`, `@budget()`, `@wcet()` | コンパイル時 | 静的 WCET 予算の宣言と整合性検証 |
| **Control Flow** | `@within(...)`, `@while(...)`, `@on_vblank:` | 実行時 | 時間制約付きブロック、イベントハンドラ |
| **Hardware Intrinsics** | `@tsc()`, `@rtt()`, `@rate()`, `@capture()`, `@send()`, `@argc()`, `@arg()` | 実行時 | ハードウェア直接実行システムコール |

---

## 5. 組み込み関数 (Hardware Intrinsics)

| 関数名 | 引数 | 戻り値 | 最悪実行時間 | 説明 |
|---|---|---|---|---|
| `@tsc()` | なし | `i64` | ~15 ns | ハードウェア TSC（`lfence; rdtsc`）のシリアル化現在値 |
| `@rtt()` | なし | `i64` (ns) | ~20 ns | ネットワーク PMD が計測した最新の RTT |
| `@rate(pct)` | `i64` (10..100) | なし | ~10 ns | 輻輳制御コントローラの送信レート変更 |
| `@capture()` | なし | `#handle` | ~100 ns | 最新の GPU フレームバッファスロットを取得 |
| `@send(#h)` | `#handle` | なし | ~200 ns | フレームを NIC 送信リングへゼロコピー送出 |
| `@argc()` | なし | `i64` | ~5 ns | スクリプトに渡された引数の個数 (0..8) |
| `@arg(idx)` | `i64` | Tagged | ~10 ns | 指定インデックスの引数をタグ付きポインタで参照 |
| `@print(val)` | 任意 | なし | ~500 ns | シリアルポートへ出力（改行なし） |
| `@println(val)`| 任意 | なし | ~500 ns | シリアルポートへ出力（改行あり） |

---

## 6. `px64` アーキテクチャ & バイトコード仕様

- **アーキテクチャ**: `px64` (Pulse Extended 64-bit Real-Time Architecture)
- **命令フォーマット**: 32-bit (4 バイト) 固定長命令
- **レジスタ数**: 20 本（16 本の GPR `$rax`〜`$r15` ＋ 4 本の HW スロット `#f0`〜`#f3`）
- **文字列プール**: 512 バイト
- **ステップ数制限**: 10,000 命令（無限ループ防止）


```
0x01: OP_PUSH_CONST   [i64]          スタックへ即値をプッシュ
0x02: OP_LOAD_VAR     [u8]           レジスタスロットからロード
0x03: OP_STORE_VAR    [u8]           スタック先頭をレジスタへストア
0x04: OP_ADD                         加算
0x05: OP_SUB                         減算
0x06: OP_MUL                         乗算
0x07: OP_DIV                         除算
0x08: OP_MOD                         剰余
0x09: OP_CMP_EQ                      一致比較
0x0A: OP_CMP_NE                      不一致比較
0x0B: OP_CMP_LT                      小なり比較
0x0C: OP_CMP_LE                      以下比較
0x0D: OP_CMP_GT                      大なり比較
0x0E: OP_CMP_GE                      以上比較
0x0F: OP_JUMP         [u16]          無条件ジャンプ
0x10: OP_JUMP_IF_FALSE [u16]         偽（0）ならジャンプ
0x11: OP_CALL_NATIVE  [u8, u8]       組み込み関数呼出 (func_id, argc)
0x12: OP_WITHIN_START [i64]          デッドラインタイマー開始 (deadline_ns)
0x13: OP_WITHIN_END                  デッドライン検証
0x14: OP_DROP                        超過フレーム破棄
0x15: OP_PUSH_STR     [u16, u16]     文字列プール参照をプッシュ
0x16: OP_HALT                        スクリプト終了
```
