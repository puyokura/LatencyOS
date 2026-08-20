# PulseLang v2 形式言語仕様書 (日本語)

---

## 1. 概要

PulseLang v2 は、LatencyOS 上で動作する超低遅延・ゼロコピーハードウェアストリーム処理のための時間駆動型 DSL です。

---

## 2. 構文構造とプレフィックス

- **変数 (`$var`)**: 静的な 64 ビット整数レジスタスロット（`$0` 〜 `$31`）にバインド。
- **ハードウェアハンドル (`#handle`)**: 複製不能な DMA 記述子（線形型）。
- **ディレクティブ・組み込み命令 (`@directive`)**: 契約、時間制御、ハードウェア直接実行関数。

---

## 3. 形式文法 (EBNF)

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

## 4. ハードウェア組み込み関数一覧

| 関数名 | シグネチャ | 最悪実行時間 | 説明 |
|---|---|---|---|
| `@tsc()` | `() -> i64` | **~15 ns** | ハードウェア TSC（`rdtscp`）のシリアル化現在値を返す |
| `@rtt()` | `() -> i64` | **~20 ns** | PMD ドライバが計測した最新の RTT（ナノ秒）を取得 |
| `@rate(pct)` | `(i64) -> ()` | **~10 ns** | 送信レート（10〜100%）を設定 |
| `@capture()` | `() -> #handle` | **~100 ns** | GPU フレームバッファの線形記述子スロットを取得 |
| `@send(#h)` | `(#handle) -> ()` | **~200 ns** | NIC 送信リングへゼロコピー送出し、所有権を移動 |
| `@print(v)` | `(Any) -> ()` | **~500 ns** | シリアル出力（改行なし） |
| `@println(v)` | `(Any) -> ()` | **~500 ns** | シリアル出力（改行あり、CRLF 自動正規化） |
