# PulseLang v3.1 & px64 v3 完全自律AI仕様書 兼 システムアーキテクチャ定義書 (日本語版)

> **ドキュメント種別**: 単一ファイル完結型 AI 形式仕様書・完全構文論・意味論 & 低レイヤアーキテクチャ定義書  
> **対象読者**: AIコーディングエージェント、LLM、静的解析器、形式検証エンジン、リアルタイムカーネルエンジニア  
> **言語バージョン**: `PulseLang v3.1 (hard-realtime DSL)`  
> **VM / ISA バージョン**: `px64 v3 Architecture (Pulse Extended 64-bit Real-Time Architecture)`  
> **実行環境**: `LatencyOS (x86_64 freestanding no_std kernel)`  
> **自己完結保証**: 本ドキュメント単体で、文法 (EBNF)・型システム・線形論理・29種の全組み込み命令・43種のpx64 v3バイトコードISA・20本レジスタマップ・メモリ順序付け・DMAコヒーレンシ・CLIツールチェーン・内蔵エディタ・43項目の形式アーキテクチャ契約・10本の標準スクリプトレシピを 100% 網羅しています。

---

# 目次

1. [言語概要と設計思想](#1-言語概要と設計思想)
2. [AI システムプロンプト & 5大不変条件 (Invariants)](#2-ai-システムプロンプト--5大不変条件-invariants)
3. [形式文法 (EBNF) 完全仕様](#3-形式文法-ebnf-完全仕様)
4. [言語仕様と全構文ルール詳解](#4-言語仕様と全構文ルール詳解)
   - [4.1 変数宣言と不変性 (let / let mut)](#41-変数宣言と不変性-let--let-mut)
   - [4.2 データ型システム](#42-データ型システム)
   - [4.3 制御構文 (if, for, while, match)](#43-制御構文-if-for-while-match)
   - [4.4 静的関数 (fn, return)](#44-静的関数-fn-return)
   - [4.5 静的構造体 (struct)](#45-静的構造体-struct)
   - [4.6 定数ルックアップテーブル (const LUT)](#46-定数ルックアップテーブル-const-lut)
   - [4.7 演算子体系と優先順位](#47-演算子体系と優先順位)
   - [4.8 文字列等値比較 (STREQ)](#48-文字列等値比較-streq)
   - [4.9 契約プログラミングと時限制御](#49-契約プログラミングと時限制御)
5. [全 Intrinsics (組み込み命令) 完全カタログ](#5-全-intrinsics-組み込み命令-完全カタログ)
   - [5.1 テレメトリ / システム管理](#51-テレメトリ--システム管理)
   - [5.2 数学 / ビット演算 / ハッシュ](#52-数学--ビット演算--ハッシュ)
   - [5.3 VRAM / 低レイヤ直接メモリアクセス](#53-vram--低レイヤ直接メモリアクセス)
   - [5.4 Tagged Result / エラーハンドリング](#54-tagged-result--エラーハンドリング)
   - [5.5 ハードウェアパイプライン](#55-ハードウェアパイプライン)
   - [5.6 シリアル出力](#56-シリアル出力)
   - [5.7 全 29 種 Intrinsics 仕様総括表](#57-全-29-種-intrinsics-仕様総括表)
6. [`px64` v3 仮想マシン ISA & バイナリ仕様](#6-px64-v3-仮想マシン-isa--バイナリ仕様)
   - [6.1 16バイト固定ヘッダー仕様](#61-16バイト固定ヘッダー仕様)
   - [6.2 20レジスタ構成マップ](#62-20レジスタ構成マップ)
   - [6.3 64ビットタグ付きポインタ仕様](#63-64ビットタグ付きポインタ仕様)
   - [6.4 32-bit 固定長命令エンコーディング](#64-32-bit-固定長命令エンコーディング)
   - [6.5 全 43 命令 Opcode 完全仕様表 (0x00 〜 0x2A)](#65-全-43-命令-opcode-完全仕様表-0x00--0x2a)
7. [開発者ツールチェーン & 開発体験](#7-開発者ツールチェーン--開発体験)
   - [7.1 `pulc` CLI コマンド・サブコマンド・フラグ](#71-pulc-cli-コマンドサブコマンドフラグ)
   - [7.2 `pulc --json` 構造化診断 JSON スキーマ](#72-pulc---json-構造化診断-json-スキーマ)
   - [7.3 `pulselang-core` クレート API](#73-pulselang-core-クレート-api)
   - [7.4 PulseEditor 操作ショートカット](#74-pulseeditor-操作ショートカット)
   - [7.5 AI-Actionable 構造化診断ログ仕様](#75-ai-actionable-構造化診断ログ仕様)
   - [7.6 主要コンパイル / 実行時エラーコード一覧](#76-主要コンパイル--実行時エラーコード一覧)
8. [低レイヤハードウェアアーキテクチャ & 43 Master Contracts](#8-低レイヤハードウェアアーキテクチャ--43-master-contracts)
9. [標準スクリプトレシピ集 (`.pul`) 実践カタログ](#9-標準スクリプトレシピ集-pul-実践カタログ)
   - [9.1 `stream.pul` (GPU-to-NIC ゼロコピーパイプライン)](#91-streampul-gpu-to-nic-ゼロコピーパイプライン)
   - [9.2 `bench.pul` (実時間演算 & レイテンシベンチマーク)](#92-benchpul-実時間演算--レイテンシベンチマーク)
   - [9.3 `filter.pul` (適応型輻輳制御ガード)](#93-filterpul-適応型輻輳制御ガード)
   - [9.4 `echo.pul` (コマンドライン引数エコー & 文字列処理)](#94-echopul-コマンドライン引数エコー--文字列処理)
   - [9.5 `math_demo.pul` (数学・ビット演算・クランプ・CRC32)](#95-math_demopul-数学ビット演算クランプcrc32)
   - [9.6 `telemetry_ext.pul` (拡張ハードウェアテレメトリ)](#96-telemetry_extpul-拡張ハードウェアテレメトリ)
   - [9.7 `vram_test.pul` (GPU フレームバッファ直接操作)](#97-vram_testpul-gpu-フレームバッファ直接操作)
   - [9.8 `fn_test.pul` (静的関数定義 & コールスタック検証)](#98-fn_testpul-静的関数定義--コールスタック検証)
   - [9.9 `struct_test.pul` (静的構造体定義 & フィールドアクセス)](#99-struct_testpul-静的構造体定義--フィールドアクセス)
   - [9.10 `match_test.pul` (Tagged Result パターンマッチング)](#910-match_testpul-tagged-result-パターンマッチング)

---

## 1. 言語概要と設計思想

**PulseLang (パルスラング) v3.1** は、LatencyOS 上で極小遅延（サブミリ秒〜マイクロ秒）のハードウェアストリーミング処理を自律制御するために設計された、**時間優先型リアクティブドメイン特化言語 (Temporal Reactive DSL)** です。

```
+-------------------------------------------------------------------------------+
|                             PulseLang v3.1 DSL                                |
|  - 時間単位の第一級サポート (ns, us, ms, s)   - 静的 WCET 解析 & 契約プログラミング    |
|  - 不変性強制 (let / let mut)               - 線形型理論 (Linear DMA Handles)       |
|  - パターンマッチング (match Ok/Err)          - 静的関数 / 構造体 / LUT               |
+---------------------------------------+---------------------------------------+
                                        | Single-Pass Compilation (pulc / core)
                                        v
+-------------------------------------------------------------------------------+
|                      px64 v3 Virtual Machine & ISA                            |
|  - 32-bit (4バイト) 固定長命令                - 20 レジスタ ($rax..$r15, #f0..#f3)    |
|  - ゼロ動的ヒープ (ゼロアロケーション)        - 64-bit タグ付きポインタ (STR/ARG/ERR)  |
|  - 10,000ステップ & 5.0ms TSC Watchdog        - 静的コールスタック (最大8深度)        |
+-------------------------------------------------------------------------------+
```

### コア設計思想:
1. **決定論的ハードリアルタイム性**:
   - すべての動的ヒープメモリ確保（`malloc` / `Box` / 動的ポインタ）を排除。ブート後は固定長配列・静的構造体・静的テーブルのみで状態を管理します。
   - コンパイル時に最悪実行時間（WCET: Worst-Case Execution Time）を算出し、ハードウェア実行時間契約（`@contract`, `@within`）と照合します。
2. **線形型システムによるリソースリークの完全防止**:
   - GPU や NIC の DMA フレームバッファは、接頭辞 `#` を持つ線形型ハンドル（`#f0`〜`#f3`）として表現されます。
   - 獲得したハンドルは、すべての実行パスで**厳密に 1 回だけ消費**（`@send` 等）されなければならず、複製や暗黙破棄はコンパイルエラーとなります。
3. **単一パス超高速コンパイルと $O(1)$ 起動**:
   - 公式ファイル拡張子は `.pul`。ホスト用スタンドアロンコンパイラ `pulc` またはカーネル内蔵コンパイラにより、直ちに実行可能バイナリ `.bin`（`px64` v3 形式）へコンパイルされます。
   - コンパイル済み `.bin` は、VM 上で $O(1)$ 定数時間で即座にディスパッチされます。
4. **共通コアクレート `pulselang-core`**:
   - コンパイラ・逆アセンブラ・検証エンジンは `pulselang-core` クレートとして一元化され、`no_std`（カーネル内蔵）、`alloc`、`std`（ホスト CLI `pulc`）のすべてのプロファイルで同一の挙動が保証されます。

---

## 2. AI システムプロンプト & 5大不変条件 (Invariants)

AI コーディングエージェントが PulseLang v3.1 のコードを生成・修正する際は、**以下の 5 つの不変条件（Invariants）を絶対的規律として厳守** してください：

```
+-----------------------------------------------------------------------------------+
|                           AI AGENT 5 INVARIANTS                                   |
+---+-----------------------------+-------------------------------------------------+
| 1 | 接頭辞規則 (曖昧性ゼロ)      | $ : 変数 ($x, $rtt, $count)                     |
|   |                             | # : ハードウェア線形ハンドル (#f, #f0..#f3)       |
|   |                             | @ : 契約・時限制御・Intrinsics (@tsc, @send)    |
+---+-----------------------------+-------------------------------------------------+
| 2 | 線形型の単一消費保証        | #f := @capture(); は全分岐で厳密に 1 回 @send   |
|   |                             | 複製・再代入・リーク・二重送信はコンパイル拒絶  |
+---+-----------------------------+-------------------------------------------------+
| 3 | 時間単位の必須明示          | 500us, 10ms, 25ns, 1s (単位のない時間数値は禁止)|
+---+-----------------------------+-------------------------------------------------+
| 4 | 文末セミコロンの必須化      | すべての文末（let, 代入, return, 関数呼び出し等）|
|   |                             | に必ず ';' を付与する                           |
+---+-----------------------------+-------------------------------------------------+
| 5 | ゼロ動的確保 & 有界実行     | ヒープ確保禁止。ループは静的有界 (for 0..N) か   |
|   |                             | 単調変化 while (最大10,000ステップ上限)         |
+---+-----------------------------+-------------------------------------------------+
```

---

## 3. 形式文法 (EBNF) 完全仕様

```ebnf
Script          ::= TopLevelDecl* <EOF>

TopLevelDecl    ::= ContractDecl
                  | PipelineDecl
                  | OnVblankDecl
                  | StructDefStmt
                  | ConstTableStmt
                  | FnDeclStmt
                  | Statement

ContractDecl    ::= "@contract:" ("@wcet(" TimeLiteral ")")? ("@budget(" TimeLiteral ")")? (";" | (ExprRelOp TimeLiteral ";"))
PipelineDecl    ::= "@pipeline:" Identifier ("@budget(" TimeLiteral ")")? (";" | Block)
OnVblankDecl    ::= "@on_vblank:" Block ";"?

Statement       ::= LetDeclStmt
                  | AssignStmt
                  | CompoundAssign
                  | MatchStmt
                  | ArrayDeclStmt
                  | ArrayAssignStmt
                  | StructDeclStmt
                  | StructAssignStmt
                  | AssertStmt
                  | ReturnStmt
                  | WithinStmt
                  | WhileStmt
                  | ForStmt
                  | IfStmt
                  | ExprStmt
                  | Block

LetDeclStmt     ::= "let" "mut"? VarIdent ( ":" TypeSpec )? ( "=" Expression )? ";"
TypeSpec        ::= Identifier | "[" "i64" ";" IntegerLiteral "]"

AssignStmt      ::= (VarIdent | HardwareIdent) ( ":=" | "=" ) Expression ";"
CompoundAssign  ::= (VarIdent | HardwareIdent) ( "+=" | "-=" ) Expression ";"

MatchStmt       ::= "match" Expression "{" MatchArm+ "}"
MatchArm        ::= Pattern "=>" ( Block | Statement ) ","?
Pattern         ::= "Ok(" VarIdent ")"
                  | "Err(" VarIdent ")"
                  | "@ok(" VarIdent ")"
                  | "@err(" VarIdent ")"
                  | "_"
                  | Expression

ConstTableStmt  ::= "const" Identifier ( ":" "[" "i64" ";" IntegerLiteral "]" )? "=" "[" ConstElemList? "]" ";"
ConstElemList   ::= ( IntegerLiteral | TimeLiteral ) ( "," ( IntegerLiteral | TimeLiteral ) )*

StructDefStmt   ::= "struct" Identifier "{" StructFieldList? "}" ";"?
StructFieldList ::= Identifier ( ":" Identifier )? ( "," Identifier ( ":" Identifier )? )*
StructDeclStmt  ::= "let" "mut"? VarIdent ":" Identifier ";"
StructAssignStmt::= VarIdent "." Identifier ( ":=" | "=" ) Expression ";"

FnDeclStmt      ::= "fn" Identifier "(" ParamList? ")" ( "->" VarIdent )? ( "@requires(" Expression ")" )* Block
ParamList       ::= VarIdent ( "," VarIdent )*
ReturnStmt      ::= "return" Expression? ";"

ArrayDeclStmt   ::= "let" "mut"? VarIdent ":" "[" "i64" ";" IntegerLiteral "]" ";"
                  | "let" "mut"? VarIdent "=" "[" ConstElemList? "]" ";"
ArrayAssignStmt ::= VarIdent "[" Expression "]" ( ":=" | "=" ) Expression ";"

AssertStmt      ::= "@assert(" Expression ")" ";"
WithinStmt      ::= "@within(" TimeLiteral ")" Block ("!drop")? ";"
WhileStmt       ::= ( "@while" | "while" ) "(" Expression ")" Block
ForStmt         ::= ( "for" | "@for" ) VarIdent "in" ( IntegerLiteral | TimeLiteral ) ".." ( IntegerLiteral | TimeLiteral ) Block
IfStmt          ::= "if" "(" Expression ")" Block ( "else" Block )?
ExprStmt        ::= Expression ";"
Block           ::= "{" Statement* "}"

Expression      ::= PipeExpr
PipeExpr        ::= TernaryExpr ( "|>" TernaryExpr )*
TernaryExpr     ::= LogicOrExpr ( "?" ( Block | Expression ) ":" ( Block | Expression ) )?
LogicOrExpr     ::= LogicAndExpr ( "||" LogicAndExpr )*
LogicAndExpr    ::= EqualityExpr ( "&&" EqualityExpr )*
EqualityExpr    ::= RelationalExpr ( ( "==" | "!=" ) RelationalExpr )*
RelationalExpr  ::= BitwiseOrExpr ( ( "<" | "<=" | ">" | ">=" ) BitwiseOrExpr )*
BitwiseOrExpr   ::= BitwiseXorExpr ( "|" BitwiseXorExpr )*
BitwiseXorExpr  ::= BitwiseAndExpr ( "^" BitwiseAndExpr )*
BitwiseAndExpr  ::= ShiftExpr ( "&" ShiftExpr )*
ShiftExpr       ::= AdditiveExpr ( ( "<<" | ">>" ) AdditiveExpr )*
AdditiveExpr    ::= Multiplicative ( ( "+" | "-" ) Multiplicative )*
Multiplicative  ::= UnaryExpr ( ( "*" | "/" | "%" ) UnaryExpr )*
UnaryExpr       ::= ( "!" | "-" )? PrimaryExpr

PrimaryExpr     ::= IntegerLiteral
                  | TimeLiteral
                  | StringLiteral
                  | VarIdent "[" Expression "]"
                  | Identifier "[" Expression "]"
                  | VarIdent "." Identifier
                  | StructInitExpr
                  | VarIdent
                  | HardwareIdent
                  | Identifier "(" ArgList? ")"
                  | IntrinsicCall
                  | "(" Expression ")"

StructInitExpr  ::= Identifier "{" ( Identifier ":" Expression ( "," Identifier ":" Expression )* )? "}"
IntrinsicCall   ::= "@" Identifier "(" ArgList? ")"
ArgList         ::= Expression ( "," Expression )*

IntegerLiteral  ::= [0-9]+ | "0x" [0-9a-fA-F]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 4. 言語仕様と全構文ルール詳解

### 4.1 変数宣言と不変性 (`let` / `let mut`)

PulseLang v3.1 は、Rust の安全哲学に準拠した**不変性デフォルト (Immutable by Default)** を採用しています。

```pulse
// 1. 不変変数の宣言 (再代入不可)
let $target_latency = 50us;
let $mask = 0xFF;

// 2. 可変変数の宣言 (let mut)
let mut $counter = 0;
$counter += 1;
$counter = $counter * 2;

// 3. レガシー互換の代入構文
// 既存スクリプトとの互換性のため := も使用可能
$counter := 10;
```

> **コンパイル時エラー検出**:
> `let $x = 10;` として宣言された不変変数に対して `$x = 20;` や `$x += 1;` などの再代入・変更を試みた場合、コンパイラは即座に `ERR_MUTABILITY_VIOLATION` を発行してコンパイルを中断します。

---

### 4.2 データ型システム

PulseLang は、動的ヒープ確保を行わない厳格な静的型システムを持ちます。

```
+------------------+-----------------------+-----------------------------------------------+
| 型名             | 構文表記例            | 内部表現 / 説明                               |
+------------------+-----------------------+-----------------------------------------------+
| 64-bit 整数      | `let $x = 100;`       | 64-bit 符号付き整数 (`i64`)                   |
| 時間型 (Time)    | `500us`, `10ms`       | 整数ナノ秒へ即座に展開 (`500_000`)            |
| 固定長配列       | `let $a: [i64; 4];`   | 静的スロット領域 (上限256要素, 境界検査付き)  |
| インライン文字列 | `let $s = "READY";`   | 512B 静的プール内のタグ付きポインタ (STR_TAG) |
| 線形ハンドル     | `#f := @capture();`   | DMA バッファ記述子スロット (#f0..#f3)         |
| Tagged Result    | `@ok($v)`, `@err($e)` | 成功/失敗ステータスを保持するタグ付き値       |
+------------------+-----------------------+-----------------------------------------------+
```

#### 固定長配列の宣言とアクセス
```pulse
// 配列の宣言 (サイズ指定)
let mut $buf: [i64; 4];

// 配列への書き込み
$buf[0] = 100;
$buf[1] = 200;
$buf[2] = 300;
$buf[3] = 400;

// 配列からの読み出し
let $val = $buf[1];
@assert($val == 200);
```

---

### 4.3 制御構文 (if, for, while, match)

#### 1. 条件分岐 (`if` / `else` および 三項演算子)
```pulse
let $rtt = @rtt();

// 標準 if-else 構文
if ($rtt > 200us) {
    @rate(60);
    @println("[ALERT] High latency detected");
} else {
    @rate(100);
}

// 三項式 / 三項ブロック
$rtt > 300us ? @rate(50) : @rate(90);
$rtt > 500us ? {
    @println("[DROP] Critical jitter");
} : {
    @println("[OK] Stable");
};
```

#### 2. 静的有界ループ (`for in 0..N`)
コンパイラが反復回数を静的に確定し、正確な WCET を算出できる推奨ループです。
```pulse
let mut $sum = 0;
for $i in 0..10 {
    $sum += $i;
}
@println($sum); // 45
```

#### 3. 条件ループ (`while`) とハードウェア保護
`while` ループは、ループ脱出条件変数が単調に変化（`$i += 1` など）していることを静的解析し、実行時には**最大 10,000 ステップ上限**および **5.0ms TSC Watchdog** によって無限ループからカーネルを保護します。
```pulse
let mut $i = 0;
let mut $accum = 1;

while ($i < 8) {
    $accum = $accum * 2;
    $i += 1;
}
@assert($accum == 256);
```

#### 4. パターンマッチング (`match`)
Result 型（`@ok`, `@err`）や整数リテラルに対する分岐を行います。
```pulse
let $res = @ok(42);

match $res {
    Ok($v) => {
        @print("Operation Succeeded: ");
        @println($v);
    },
    Err($e) => {
        @print("Operation Failed with error: ");
        @println($e);
    },
    _ => {
        @println("Unknown state");
    },
};
```

---

### 4.4 静的関数 (`fn`, `return`)

PulseLang v3.1 は、最大 8 フレームの静的コールスタック（`MAX_CALL_DEPTH = 8`）を持つ関数定義をサポートします。

```pulse
// 関数の定義 (引数と戻り値)
fn compute_penalty($rtt_ns, $loss_pct) {
    let $base = $rtt_ns / 1000;
    let $penalty = $base * $loss_pct;
    return $penalty;
}

// 関数の呼び出し
let $p = compute_penalty(50000, 5);
@println($p);
```

> **スタック制約**: 再帰呼び出しや深さ 8 を超える多段呼び出しが発生した場合、VM は `ERR_PX64_STACK_OVERFLOW` を発出して安全に停止します。

---

### 4.5 静的構造体 (`struct`)

ヒープを一切使わずに、固定スロット領域上に複数のフィールドを持つデータ構造を定義・操作できます。

```pulse
// 構造体の型定義
struct TelemetryFrame {
    seq_id: i64,
    timestamp: i64,
    rtt_ns: i64,
    loss_rate: i64,
};

// 構造体インスタンスの宣言
let mut $frame: TelemetryFrame;

// フィールドへの代入
$frame.seq_id = 1;
$frame.timestamp = @tsc();
$frame.rtt_ns = @rtt();
$frame.loss_rate = 0;

// フィールドの読み出し
let $current_seq = $frame.seq_id;
@println($current_seq);
```

---

### 4.6 定数ルックアップテーブル (`const LUT`)

定数テーブルはバイナリの定数プール領域に配置され、$O(1)$ かつキャッシュフレンドリにアクセスされます。

```pulse
// 定数ルックアップテーブルの定義 (4要素)
const SPEED_LUT: [i64; 4] = [0, 64, 128, 255];

let $gear = 2;
let $throttle = SPEED_LUT[$gear];
@assert($throttle == 128);
```

---

### 4.7 演算子体系と優先順位

演算子は標準的な C/Rust 優先順位に準拠し、64ビットの 2 の補数演算でオーバーフローラップします。

| 優先度 | 分類 | 演算子 | 説明 |
|---|---|---|---|
| 1 (高) | 単項 | `!`, `-` | 論理否定, 算術反転 |
| 2 | 乗除余 | `*`, `/`, `%` | 整数乗算, 除算 (0除算保護), 剰余 |
| 3 | 加減 | `+`, `-` | 整数加算, 整数減算 |
| 4 | シフト | `<<`, `>>` | 論理左シフト, 論理右シフト |
| 5 | ビットAND | `&` | ビット単位論理積 |
| 6 | ビットXOR | `^` | ビット単位排他的論理和 |
| 7 | ビットOR | `\|` | ビット単位論理和 |
| 8 | 比較 | `<`, `<=`, `>`, `>=` | 大小比較 (真: 1, 偽: 0) |
| 9 | 等値 | `==`, `!=` | 一致・不一致比較 |
| 10 | 論理AND | `&&` | 短絡論理積 |
| 11 | 論理OR | `\|\|` | 短絡論理和 |
| 12 | 三項 | `? :` | 条件分岐演算子 |
| 13 (低)| パイプ | `\|>` | ストリームパイプライン |

> **0 除算保護**: `100 / 0` や `100 % 0` を実行した場合、CPU トラップを起こさず安全に `0` を返します。

---

### 4.8 文字列等値比較 (STREQ)

PulseLang は文字列ポインタ間の $O(1)$ 有界な等値比較演算を直接サポートします。

```pulse
let $s1 = "LATENCY_OS";
let $s2 = "LATENCY_OS";

if ($s1 == $s2) {
    @println("[MATCH] String signatures are identical.");
}
```

---

### 4.9 契約プログラミングと時限制御

```pulse
// 1. スクリプト全体の WCET / 予算契約
@contract: @wcet(5us) @budget(50us);

// 2. パイプライン定義
@pipeline: UltraStream @budget(8000us);

// 3. VBLANK 垂直同期ブロック
@on_vblank: {
    #f := @capture();
    
    // 4. マイクロ秒時限ガード (!drop による過渡パケット破棄)
    @within(500us) {
        $rtt := @rtt();
        $rtt > 200us ? @rate(80) : @rate(100);
        @send(#f);
    } !drop;
};

// 5. 事前条件 & アサーション
@assert($rtt >= 0);
```

---

## 5. 全 Intrinsics (組み込み命令) 完全カタログ

PulseLang v3.1 には、ハードウェアおよびカーネルに直結した **全 29 種類の Intrinsics** が標準装備されています。

### 5.1 テレメトリ / システム管理

#### 1. `@core_id()`
- **シグネチャ**: `() -> i64` | **WCET**: `~5 ns`
- **説明**: 現在コードを実行している CPU コアの Local APIC ID（0, 1, 2, 3）を返します。
- **使用例**: `let $cid = @core_id();`

#### 2. `@tsc_freq()`
- **シグネチャ**: `() -> i64` | **WCET**: `~5 ns`
- **説明**: カーネル起動時にキャリブレーションされた TSC 周波数（MHz 単位、例: 3400 = 3.4GHz）を返します。
- **使用例**: `let $mhz = @tsc_freq();`

#### 3. `@uptime_ns()`
- **シグネチャ**: `() -> i64` | **WCET**: `~20 ns`
- **説明**: システム起動（ブート）時からの通算経過時間をナノ秒単位で返します。
- **使用例**: `let $up = @uptime_ns();`

#### 4. `@busy_wait($ns)`
- **シグネチャ**: `(i64) -> 0` | **WCET**: `引数 + ~15 ns`
- **説明**: 指定されたナノ秒間、CPU の TSC カウンタを監視しながら高精度にスピンループ待機します。
- **使用例**: `@busy_wait(500ns);`

#### 5. `@ring_depth($ring_id)`
- **シグネチャ**: `(i64) -> i64` | **WCET**: `~10 ns`
- **説明**: カーネルの SPSC ロックフリーリングバッファの現在キュー長を取得します。
  - `$ring_id = 0`: GPU Capture $\to$ Encode リング
  - `$ring_id = 1`: Encode $\to$ Network TX リング
- **使用例**: `let $depth = @ring_depth(0);`

#### 6. `@tsc()`
- **シグネチャ**: `() -> i64` | **WCET**: `~15 ns`
- **説明**: `lfence; rdtsc` を発行し、シリアル化された現在の 64-bit TSC クロックサイクル数を返します。
- **使用例**: `let $t0 = @tsc();`

#### 7. `@argc()`
- **シグネチャ**: `() -> i64` | **WCET**: `~5 ns`
- **説明**: `run <script.pul> [args...]` 実行時にシェルから渡された引数の個数（0〜8）を返します。
- **使用例**: `let $n = @argc();`

#### 8. `@arg($idx)`
- **シグネチャ**: `(i64) -> Tagged` | **WCET**: `~10 ns`
- **説明**: コマンドライン引数スロット（0〜7）の文字列をゼロコピー参照タグ付きポインタ（`ARG_TAG`）として返します。
- **使用例**: `@println(@arg(0));`

---

### 5.2 数学 / ビット演算 / ハッシュ

#### 9. `@min($a, $b)`
- **シグネチャ**: `(i64, i64) -> i64` | **WCET**: `~2 ns`
- **説明**: 2 つの整数のうち小さい方を返します。
- **使用例**: `let $m = @min($x, 100);`

#### 10. `@max($a, $b)`
- **シグネチャ**: `(i64, i64) -> i64` | **WCET**: `~2 ns`
- **説明**: 2 つの整数のうち大きい方を返します。
- **使用例**: `let $m = @max($x, 0);`

#### 11. `@abs($a)`
- **シグネチャ**: `(i64) -> i64` | **WCET**: `~2 ns`
- **説明**: 整数の絶対値を返します（飽和演算付き）。
- **使用例**: `let $diff = @abs($t2 - $t1);`

#### 12. `@clamp($v, $min, $max)`
- **シグネチャ**: `(i64, i64, i64) -> i64` | **WCET**: `~4 ns`
- **説明**: 値 `$v` を `[$min, $max]` の範囲内に制限して返します。
- **使用例**: `let $safe_rate = @clamp($calc_rate, 10, 100);`

#### 13. `@popcnt($v)`
- **シグネチャ**: `(i64) -> i64` | **WCET**: `~2 ns`
- **説明**: 64-bit 整数の中で立っているビット数（1 の個数、`POPCNT` 命令相当）を返します。
- **使用例**: `let $bits = @popcnt($bitmask);`

#### 14. `@lzcnt($v)`
- **シグネチャ**: `(i64) -> i64` | **WCET**: `~2 ns`
- **説明**: 64-bit 整数の最上位ビットからの先行ゼロの個数（`LZCNT` 命令相当）を返します。
- **使用例**: `let $leading_zeros = @lzcnt($val);`

#### 15. `@crc32($seed, $val)`
- **シグネチャ**: `(i64, i64) -> i64` | **WCET**: `~5 ns`
- **説明**: 64-bit 整数 `$val` に対して、初期シード `$seed` を用いてハードウェア CRC32-C チェックサムを計算します。
- **使用例**: `let $checksum = @crc32(0xFFFFFFFF, $data);`

---

### 5.3 VRAM / 低レイヤ直接メモリアクセス

#### 16. `@vram_read($slot, $offset)`
- **シグネチャ**: `(i64, i64) -> i64` | **WCET**: `~8 ns`
- **説明**: GPU フレームバッファスロット `$slot`（0〜3）のオフセット `$offset` バイト位置から 64-bit/32-bit のピクセルデータを直接読み出します。
- **使用例**: `let $pixel = @vram_read(0, 1024);`

#### 17. `@vram_write($slot, $offset, $val)`
- **シグネチャ**: `(i64, i64, i64) -> 0` | **WCET**: `~8 ns`
- **説明**: GPU フレームバッファスロット `$slot`（0〜3）のオフセット `$offset` バイト位置へ 64-bit/32-bit 値を直接書き込みます。
- **使用例**: `@vram_write(0, 1024, 0x00FF00FF);`

---

### 5.4 Tagged Result / エラーハンドリング

#### 18. `@ok($val)`
- **シグネチャ**: `(i64) -> Tagged` | **WCET**: `~2 ns`
- **説明**: 成功値 `$val` をカプセル化した Tagged Result を生成します。
- **使用例**: `let $res = @ok(200);`

#### 19. `@err($code)`
- **シグネチャ**: `(i64) -> Tagged` | **WCET**: `~2 ns`
- **説明**: エラーコード `$code`（`ERR_TAG: 0x1000_...` 付与）を持つ Tagged Result を生成します。
- **使用例**: `let $res = @err(404);`

#### 20. `@is_ok($res)`
- **シグネチャ**: `(Tagged) -> 1/0` | **WCET**: `~2 ns`
- **説明**: Result が成功値であれば `1`、エラーであれば `0` を返します。
- **使用例**: `if (@is_ok($res)) { ... }`

#### 21. `@is_err($res)`
- **シグネチャ**: `(Tagged) -> 1/0` | **WCET**: `~2 ns`
- **説明**: Result がエラー値であれば `1`、成功値であれば `0` を返します。
- **使用例**: `if (@is_err($res)) { ... }`

#### 22. `@unwrap($res)`
- **シグネチャ**: `(Tagged) -> i64` | **WCET**: `~3 ns`
- **説明**: 成功値を取り出します。エラー値に対して呼び出された場合、`ERR_PX64_UNWRAP_FAILED` で VM は安全に停止します。
- **使用例**: `let $val = @unwrap($res);`

---

### 5.5 ハードウェアパイプライン

#### 23. `@capture()`
- **シグネチャ**: `() -> #handle` | **WCET**: `~100 ns`
- **説明**: VBLANK 同期した GPU キャプチャリングバッファのスロットをゼロコピーで獲得し、線形ハンドルを返します。
- **使用例**: `#f := @capture();`

#### 24. `@send(#handle)`
- **シグネチャ**: `(#handle) -> 1` | **WCET**: `~200 ns`
- **説明**: 獲得したフレームバッファを Intel e1000 NIC の TX リングへエンキューし、ハードウェアへ送信すると共に線形所有権を消費します。
- **使用例**: `@send(#f);`

#### 25. `@rtt()`
- **シグネチャ**: `() -> i64` | **WCET**: `~20 ns`
- **説明**: Intel e1000 NIC ドライバが計測・追従している直近の最小ネットワーク往復時間（RTT）をナノ秒単位で返します。
- **使用例**: `let $rtt_ns = @rtt();`

#### 26. `@rate($pct)`
- **シグネチャ**: `(i64) -> 0` | **WCET**: `~10 ns`
- **説明**: 輻輳制御エンジンに対する送信帯域レートパーセンテージ（10%〜100%）を設定します。
- **使用例**: `@rate(80);`

---

### 5.6 シリアル出力

#### 27. `@print($val)`
- **シグネチャ**: `(Any) -> 0` | **WCET**: `~500 ns`
- **説明**: 文字列リテラル、引数参照、または 64-bit 整数を UART COM1 シリアルポートへ出力します（改行なし）。
- **使用例**: `@print("Value: ");`

#### 28. `@println($val)`
- **シグネチャ**: `(Any) -> 0` | **WCET**: `~500 ns`
- **説明**: 値を出力したのち、自動的に `\r\n` (CRLF) をシリアルポートへ出力します。
- **使用例**: `@println($result);`

#### 29. `@streq($s1, $s2)`
- **シグネチャ**: `(str, str) -> 1/0` | **WCET**: `~5 ns`
- **説明**: 2 つの文字列または引数参照が完全に一致するかを判定します（一致なら 1、不一致なら 0）。
- **使用例**: `let $match = @streq($s1, "OK");`

---

### 5.7 全 29 種 Intrinsics 仕様総括表

| ID | 組み込み関数名 | 引数シグネチャ | 戻り値 | 最悪実行時間 (WCET) | 分類 |
|---|---|---|---|---|---|
| `1` | `@print` | `(val: any)` | `0` | ~500 ns | 出力 |
| `2` | `@println` | `(val: any)` | `0` | ~500 ns | 出力 |
| `3` | `@tsc` | `()` | `i64` | ~15 ns | システム |
| `4` | `@rtt` | `()` | `i64` | ~20 ns | ネットワーク |
| `5` | `@rate` | `(pct: i64)` | `0` | ~10 ns | 輻輳制御 |
| `6` | `@capture` | `()` | `#handle` | ~100 ns | GPU |
| `7` | `@send` | `(handle: #handle)`| `1` | ~200 ns | ネットワーク |
| `8` | `@argc` | `()` | `i64` | ~5 ns | CLI引数 |
| `9` | `@arg` | `(idx: i64)` | `Tagged` | ~10 ns | CLI引数 |
| `10`| `@ok` | `(val: i64)` | `Tagged` | ~2 ns | Result |
| `11`| `@err` | `(code: i64)` | `Tagged` | ~2 ns | Result |
| `12`| `@is_ok` | `(res: Tagged)` | `1 / 0` | ~2 ns | Result |
| `13`| `@is_err` | `(res: Tagged)` | `1 / 0` | ~2 ns | Result |
| `14`| `@unwrap` | `(res: Tagged)` | `i64` | ~3 ns | Result |
| `15`| `@streq` | `(s1: str, s2: str)`| `1 / 0` | ~5 ns | 文字列 |
| `16`| `@core_id` | `()` | `i64` | ~5 ns | システム |
| `17`| `@tsc_freq` | `()` | `i64` | ~5 ns | システム |
| `18`| `@uptime_ns`| `()` | `i64` | ~20 ns | システム |
| `19`| `@busy_wait`| `(ns: i64)` | `0` | 引数 + ~15 ns | タイマー |
| `20`| `@ring_depth`| `(ring_id: i64)` | `i64` | ~10 ns | SPSCキュー |
| `21`| `@min` | `(a: i64, b: i64)` | `i64` | ~2 ns | 数学 |
| `22`| `@max` | `(a: i64, b: i64)` | `i64` | ~2 ns | 数学 |
| `23`| `@abs` | `(a: i64)` | `i64` | ~2 ns | 数学 |
| `24`| `@clamp` | `(v, min, max)` | `i64` | ~4 ns | 数学 |
| `25`| `@popcnt` | `(v: i64)` | `i64` | ~2 ns | ビット演算 |
| `26`| `@lzcnt` | `(v: i64)` | `i64` | ~2 ns | ビット演算 |
| `27`| `@crc32` | `(seed, val)` | `i64` | ~5 ns | ハッシュ |
| `28`| `@vram_read`| `(slot, offset)` | `i64` | ~8 ns | VRAM |
| `29`| `@vram_write`| `(slot, offset, val)`| `0` | ~8 ns | VRAM |

---

## 6. `px64` v3 仮想マシン ISA & バイナリ仕様

### 6.1 16バイト固定ヘッダー仕様

すべての `px64` v3 実行可能バイナリ（`.bin`）は、先頭に厳格な 16 バイトのバイナリヘッダーを持ちます。

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       Magic: 'P' 'X' '6' '4' (0x50, 0x58, 0x36, 0x34)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      Version: 3 (0x0003)      |      CodeLen (u16 Big-Endian) |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   StrPoolLen (u16 Big-Endian) |  ConstPoolLen (u16 Big-Endian)|
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   NumRegs: 20 (0x0014)        |      Reserved (0x0000)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
|    Code Payload (CodeLen Bytes: 4-Byte Aligned Instructions)  |
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    String Pool Payload (StrPoolLen Bytes: UTF-8 String Data)  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Const Pool Payload (ConstPoolLen * 8 Bytes: 64-bit Values) |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

---

### 6.2 20レジスタ構成マップ

`px64` 仮想マシンは、**16 本の 64-bit 汎用レジスタ（GPR）** と **4 本のハードウェア DMA スロットレジスタ** の計 20 レジスタ構成を持ちます。

```
+----+--------+---------------+-------------------------------------------------+
| ID | レジスタ| x64 互換名    | 用途・セマンティクス                            |
+----+--------+---------------+-------------------------------------------------+
| 0  | $rax   | $r0           | アキュムレータ, 式評価結果, 関数戻り値          |
| 1  | $rcx   | $r1           | カウンタ, ユーザー変数スロット 1                |
| 2  | $rdx   | $r2           | データ, ユーザー変数スロット 2                  |
| 3  | $rbx   | $r3           | ベース, ユーザー変数スロット 3                  |
| 4  | $rsp   | $r4           | スタックポインタ名, ユーザー変数スロット 4      |
| 5  | $rbp   | $r5           | ベースポインタ名, ユーザー変数スロット 5        |
| 6  | $rsi   | $r6           | ソースインデックス, ユーザー変数スロット 6      |
| 7  | $rdi   | $r7           | デスティネーション, ユーザー変数スロット 7      |
| 8  | $r8    | $r8           | ユーザー変数スロット 8                          |
| 9  | $r9    | $r9           | ユーザー変数スロット 9                          |
| 10 | $r10   | $r10          | ユーザー変数スロット 10                         |
| 11 | $r11   | $r11          | ユーザー変数スロット 11                         |
| 12 | $r12   | $r12          | ユーザー変数スロット 12                         |
| 13 | $r13   | $r13          | ユーザー変数スロット 13                         |
| 14 | $r14   | $r14          | 内部コンパイラ・セカンダリ一時スクラッチ        |
| 15 | $r15   | $r15          | 内部コンパイラ・プライマリ一時スクラッチ        |
| 16 | #f0    | #frame0       | ハードウェア DMA キャプチャスロット 0           |
| 17 | #f1    | #frame1       | ハードウェア DMA キャプチャスロット 1           |
| 18 | #f2    | #frame2       | ハードウェア DMA キャプチャスロット 2           |
| 19 | #f3    | #frame3       | ハードウェア DMA キャプチャスロット 3           |
+----+--------+---------------+-------------------------------------------------+
```

---

### 6.3 64ビットタグ付きポインタ仕様

ヒープ動的確保を一切行わないため、参照値は上位ビットマスクでタグ付けされます：

```text
63 62 61 60 59                                                    0
+--+--+--+--+-----------------------------------------------------+
|S |A |E |  | Data Payload (Offset, Length, Index, Value)         |
+--+--+--+--+-----------------------------------------------------+
 S (Bit 62): STR_TAG (0x4000_0000_0000_0000) -> 文字列プール (Offset << 32 | Len)
 A (Bit 61): ARG_TAG (0x2000_0000_0000_0000) -> CLI 引数スロット (0..7)
 E (Bit 60): ERR_TAG (0x1000_0000_0000_0000) -> Result エラー状態
```

---

### 6.4 32-bit 固定長命令エンコーディング

すべての `px64` 命令は厳密に 4 バイト境界に配置されます。

```text
Byte 0: Opcode (0x00 .. 0x2A)
Byte 1: Rd (宛先レジスタ / 識別子)
Byte 2: Rs1 (第1ソースレジスタ / 即値上位 / FuncId)
Byte 3: Rs2 (第2ソースレジスタ / 即値下位 / ArgReg)
```

---

### 6.5 全 43 命令 Opcode 完全仕様表 (0x00 〜 0x2A)

| Hex | Opcode 定数名 | ニーモニック | オペランド | 詳細動作・セマンティクス | WCET |
|---|---|---|---|---|---|
| `0x00` | `PX64_OP_NOP` | `NOP` | なし | 何もしない (No Operation) | ~1 ns |
| `0x01` | `PX64_OP_MOV_IMM` | `MOV` | `Rd, Imm16` | 16-bit 即値を Rd に代入 (`Rd = (Ih << 8) \| Il`) | ~2 ns |
| `0x02` | `PX64_OP_MOV_REG` | `MOV` | `Rd, Rs1` | Rs1 の値を Rd に転送 (`Rd = Rs1`) | ~2 ns |
| `0x03` | `PX64_OP_MOV_STR` | `MOVS` | `Rd, Off, Len` | 文字列タグ付きポインタを Rd に代入 | ~3 ns |
| `0x04` | `PX64_OP_ADD` | `ADD` | `Rd, Rs1, Rs2` | 64-bit 整数加算 (`Rd = Rs1 + Rs2`) | ~2 ns |
| `0x05` | `PX64_OP_SUB` | `SUB` | `Rd, Rs1, Rs2` | 64-bit 整数減算 (`Rd = Rs1 - Rs2`) | ~2 ns |
| `0x06` | `PX64_OP_MUL` | `MUL` | `Rd, Rs1, Rs2` | 64-bit 整数乗算 (`Rd = Rs1 * Rs2`) | ~3 ns |
| `0x07` | `PX64_OP_DIV` | `DIV` | `Rd, Rs1, Rs2` | 64-bit 整数除算 (0除算保護: Rs2==0なら0) | ~12 ns |
| `0x08` | `PX64_OP_MOD` | `MOD` | `Rd, Rs1, Rs2` | 64-bit 整数剰余 (0除算保護: Rs2==0なら0) | ~12 ns |
| `0x09` | `PX64_OP_CMP_EQ` | `CMPEQ` | `Rd, Rs1, Rs2` | 一致比較 (`Rd = (Rs1 == Rs2) ? 1 : 0`) | ~2 ns |
| `0x0A` | `PX64_OP_CMP_NE` | `CMPNE` | `Rd, Rs1, Rs2` | 不一致比較 (`Rd = (Rs1 != Rs2) ? 1 : 0`) | ~2 ns |
| `0x0B` | `PX64_OP_CMP_LT` | `CMPLT` | `Rd, Rs1, Rs2` | 小なり比較 (`Rd = (Rs1 < Rs2) ? 1 : 0`) | ~2 ns |
| `0x0C` | `PX64_OP_CMP_LE` | `CMPLE` | `Rd, Rs1, Rs2` | 以下比較 (`Rd = (Rs1 <= Rs2) ? 1 : 0`) | ~2 ns |
| `0x0D` | `PX64_OP_CMP_GT` | `CMPGT` | `Rd, Rs1, Rs2` | 大なり比較 (`Rd = (Rs1 > Rs2) ? 1 : 0`) | ~2 ns |
| `0x0E` | `PX64_OP_CMP_GE` | `CMPGE` | `Rd, Rs1, Rs2` | 以上比較 (`Rd = (Rs1 >= Rs2) ? 1 : 0`) | ~2 ns |
| `0x0F` | `PX64_OP_JMP` | `JMP` | `Target16` | 無条件ジャンプ (`IP = Target`) | ~2 ns |
| `0x10` | `PX64_OP_JZ` | `JZ` | `Rs1, Target16` | ゼロ判定ジャンプ (`Rs1 == 0` なら `IP = Target`) | ~3 ns |
| `0x11` | `PX64_OP_JNZ` | `JNZ` | `Rs1, Target16` | 非ゼロ判定ジャンプ (`Rs1 != 0` なら `IP = Target`) | ~3 ns |
| `0x12` | `PX64_OP_CALL_NAT` | `CALL_NAT`| `Rd, FnId, Arg` | ハードウェア組み込み命令呼出 (`Rd = call_native(Fn, Arg)`) | 処理依存 |
| `0x13` | `PX64_OP_WITHIN_START`|`WITHIN_START`|`Rs1` | 時限デッドライン開始 (Rs1: マイクロ秒単位) | ~15 ns |
| `0x14` | `PX64_OP_WITHIN_END` |`WITHIN_END` | なし | デッドラインスタックをポップ | ~2 ns |
| `0x15` | `PX64_OP_DROP` | `DROP` | なし | 時限超過時に未送信フレームを安全破棄 | ~10 ns |
| `0x16` | `PX64_OP_HALT` | `HALT` | なし | VM の実行を正常終了 | ~1 ns |
| `0x17` | `PX64_OP_LDC` | `LDC` | `Rd, ConstIdx16`| 64-bit 定数ロード (`Rd = const_pool[ConstIdx]`) | ~2 ns |
| `0x18` | `PX64_OP_ADDI` | `ADDI` | `Rd, Rs1, Imm8`| 8-bit 即値加算 (`Rd = Rs1 + Imm8`) | ~2 ns |
| `0x19` | `PX64_OP_SUBI` | `SUBI` | `Rd, Rs1, Imm8`| 8-bit 即値減算 (`Rd = Rs1 - Imm8`) | ~2 ns |
| `0x1A` | `PX64_OP_AND` | `AND` | `Rd, Rs1, Rs2` | ビット単位 AND (`Rd = Rs1 & Rs2`) | ~2 ns |
| `0x1B` | `PX64_OP_OR` | `OR` | `Rd, Rs1, Rs2` | ビット単位 OR (`Rd = Rs1 \| Rs2`) | ~2 ns |
| `0x1C` | `PX64_OP_XOR` | `XOR` | `Rd, Rs1, Rs2` | ビット単位 XOR (`Rd = Rs1 ^ Rs2`) | ~2 ns |
| `0x1D` | `PX64_OP_SHL` | `SHL` | `Rd, Rs1, Rs2` | 64-bit 左シフト (`Rd = Rs1 << (Rs2 & 63)`) | ~2 ns |
| `0x1E` | `PX64_OP_SHR` | `SHR` | `Rd, Rs1, Rs2` | 64-bit 論理右シフト (`Rd = Rs1 as u64 >> (Rs2 & 63)`) | ~2 ns |
| `0x1F` | `PX64_OP_ARR_DEF` | `ARR_DEF` | `ArrId, Len16` | 静的配列の長さ定義 (`array_lens[ArrId] = Len`) | ~3 ns |
| `0x20` | `PX64_OP_ARR_LOAD` | `ARR_LOAD`| `Rd, ArrId, Rs` | 境界チェック付き配列読出 (`Rd = array_slots[base + Rs]`) | ~4 ns |
| `0x21` | `PX64_OP_ARR_STORE`| `ARR_STORE`|`ArrId, Rs1, Rs2`| 境界チェック付き配列書込 (`array_slots[base + Rs1] = Rs2`) | ~4 ns |
| `0x22` | `PX64_OP_ASSERT` | `ASSERT` | `Rs1` | アサーション検証 (Rs1 == 0 なら ASSERTION_FAILED) | ~2 ns |
| `0x23` | `PX64_OP_CALL` | `CALL` | `Target16` | 戻り先IP退避して関数ジャンプ (コールスタック上限8) | ~4 ns |
| `0x24` | `PX64_OP_RET` | `RET` | なし | 戻り先IPへ復帰 (`$rax` に戻り値を保持) | ~4 ns |
| `0x25` | `PX64_OP_STRUCT_DEF`|`STRUCT_DEF`|`InstId, FCount`| 構造体インスタンス定義 (`struct_field_counts[InstId] = FCount`) | ~2 ns |
| `0x26` | `PX64_OP_STRUCT_LOAD`|`STRUCT_LOAD`|`Rd, InstId, Of`| 構造体フィールド読出 (`Rd = struct_slots[base + Of]`) | ~3 ns |
| `0x27` | `PX64_OP_STRUCT_STORE`|`STRUCT_STORE`|`InstId, Of, Rs`| 構造体フィールド書込 (`struct_slots[base + Of] = Rs`) | ~3 ns |
| `0x28` | `PX64_OP_TBL_DEF` | `TBL_DEF` | `TblId, Ba, Le` | 定数テーブル定義 (`table_bases[TblId] = Ba, lens = Le`) | ~2 ns |
| `0x29` | `PX64_OP_TBL_LOAD` | `TBL_LOAD`| `Rd, TblId, Rs` | 境界チェック付きテーブル読出 (`Rd = const_pool[base + Rs]`) | ~3 ns |
| `0x2A` | `PX64_OP_STREQ` | `STREQ` | `Rd, Rs1, Rs2` | 有界 $O(1)$ 文字列等値比較 (`Rd = (streq(Rs1, Rs2)) ? 1 : 0`) | ~5 ns |

---

## 7. 開発者ツールチェーン & 開発体験

### 7.1 `pulc` CLI コマンド・サブコマンド・フラグ

`pulc` は、ホスト環境（Linux / macOS / Windows）で PulseLang スクリプトのコンパイル・静的検証・逆アセンブルを行う公式 CLI ツールです。

```text
USAGE:
    pulc <file.pul> [-o <out.bin>]
    pulc compile <file.pul> [-o <out.bin>]
    pulc check <file.pul>
    pulc disasm <file.bin>
    pulc -d <file.bin>

SUBCOMMANDS:
    compile <file.pul>    PulseLang ソースを px64 バイナリバイトコードへコンパイル
    check <file.pul>      構文・型・不変性・線形所有権・静的 WCET を完全検証 (コード生成なし)
    disasm <file.bin>     px64 バイナリファイルを可読なアセンブリ命令一覧へ逆アセンブル

FLAGS:
    -o, --output <file>   出力バイナリファイルパスを指定 (デフォルト: <input>.bin)
    -d, --disasm          バイナリファイルの逆アセンブルを実行
    --json                AI エージェント / CI 用に構造化 JSON フォーマットで結果を出力
    -v, --verbose         詳細な診断ログを出力
    -h, --help            ヘルプメッセージを出力
    -V, --version         バージョン情報を出力

EXIT CODES:
    0   Success (正常終了)
    1   Compilation, syntax, linear ownership, mutability, or WCET violation error
    2   IO, file access, or command-line argument error
```

---

### 7.2 `pulc --json` 構造化診断 JSON スキーマ

AI エージェントおよび自動化パイプラインは、`--json` フラグを付与することで、標準出力から直接 JSON をパースしてエラー修復やバイナリ生成ステータスを判定できます。

#### 1. 成功時 (Success Schema)
```json
{
  "success": true,
  "file": "stream.pul",
  "output_file": "stream.bin",
  "code_size_bytes": 128,
  "instruction_count": 28,
  "string_pool_bytes": 42,
  "const_pool_entries": 3,
  "wcet_ns": 4850,
  "diagnostics": []
}
```

#### 2. 失敗時 (Failure / Diagnostic Schema)
```json
{
  "success": false,
  "file": "faulty.pul",
  "error": {
    "code": "ERR_MUTABILITY_VIOLATION",
    "message": "Cannot mutate immutable variable declared with 'let'",
    "line": 3,
    "col": 5,
    "byte_offset": 45,
    "token_kind": "VarIdent",
    "token_len": 4,
    "expected": "Mutable variable declared with 'let mut'",
    "stage": "Statement -> Assignment",
    "suggestion": "Change variable declaration to 'let mut $count = ...;'",
    "ai_repair_hint": "Add 'mut' keyword to the variable declaration or avoid re-assignment"
  }
}
```

---

### 7.3 `pulselang-core` クレート API

Rust プログラムやカーネル内から直接 PulseLang を利用するための API です。

```rust
use pulselang_core::{compile, compile_pulse_to_binary, check, disassemble_px64_with_filename};

// 1. 静的検証のみを実行
let stats = check(source_code_str)?;
println!("WCET: {} ns, Instructions: {}", stats.wcet_ns, stats.instruction_count);

// 2. メモリ上へコンパイル (alloc / std 環境)
let binary_bytes = compile(source_code_str)?;

// 3. 固定長バッファへゼロアロケーションコンパイル (no_std カーネル環境)
let mut bin_buf = [0u8; 1024];
let bin_size = compile_pulse_to_binary(source_code_str.as_bytes(), &mut bin_buf)?;

// 4. 逆アセンブル文字列の生成
let mut disasm_out = String::new();
disassemble_px64_with_filename(&bin_buf[..bin_size], "stream.bin", &mut disasm_out)?;
```

---

### 7.4 PulseEditor 操作ショートカット

LatencyOS カーネル内蔵の ANSI フルスクリーンエディタ **PulseEditor** のキーバインドです。

| ショートカット | ファンクションキー | 機能名 | 説明 |
|---|---|---|---|
| `Ctrl + S` | `F2` | **Save** | 編集中のバッファを LatencyFS へ即座に保存 |
| `Ctrl + R` | `F5` | **Run** | エディタを終了せずにスクリプトを即時コンパイル & 実行 |
| `Ctrl + Q` | `F10` | **Quit** | 保存せずにエディタを終了し Pulse Shell へ復帰 |
| `Ctrl + X` | - | **Save & Quit** | バッファを保存して即座にシェルへ復帰 |
| `Esc C` / `Ctrl + C` | - | **Clear** | 編集バッファを一括消去 |
| `Ctrl + A` | `Home` | **Home** | カーソルを現在行の先頭へ移動 |
| `Ctrl + E` | `End` | **End** | カーソルを現在行の末尾へ移動 |
| `Ctrl + K` | - | **Kill Line** | カーソル位置から行末までを一括削除 |
| `Ctrl + U` | - | **Kill to Start** | カーソル位置から行頭までを一括削除 |
| `Ctrl + D` | `Delete` | **Delete** | カーソル位置の 1 文字を削除 |
| `Ctrl + L` | - | **Redraw** | 画面全体を強制再描画 |

---

### 7.5 AI-Actionable 構造化診断ログ仕様

PulseLang コンパイラおよび VM は、エラー発生時に AI エージェントが 1 回の推論で完全修復を行える構造化ログを出力します：

```text
==================== [PULSELANG COMPILE ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================
[ERROR_CODE]: ERR_LINEAR_UNCONSUMED_HANDLE
[MESSAGE]: Linear hardware handle acquired but not consumed in all execution branches
[FILE]: /home/stream.pul
[LOCATION]: Line 4, Column 5 (ByteOffset: 72)
[TOKEN_FOUND]: Kind: HardwareIdent, Value: "#f0"
[EXPECTED]: Consumption of handle via '@send(#f0);'
[PARSER_STAGE]: Linear Ownership Static Verification
[SOURCE_CONTEXT]:
  Line   3: @on_vblank: {
> Line   4:     #f0 := @capture();
                ^^^ [Linear Resource Leak Detected]
  Line   5: };
[HEX_DUMP (offset 0x0040..0x0055)]:
  00000040: 23 66 30 20 3a 3d 20 40 63 61 70 74 75 72 65 28  |#f0 := @capture(|
  00000050: 29 3b 0a 7d 3b                                    |);.};|
[AI_REPAIR_HINT]: Ensure '@send(#f0);' is called on all execution paths before exiting block
=============================================================================================
```

---

### 7.6 主要コンパイル / 実行時エラーコード一覧

```
+------------------------------------+---------------------------------------------------------------+
| エラーコード                       | 根本原因と AI 修復指針 (AI Repair Action)                     |
+------------------------------------+---------------------------------------------------------------+
| ERR_MUTABILITY_VIOLATION           | let で宣言された不変変数への再代入 -> 'let mut' を付与        |
| ERR_UNBOUNDED_LOOP                 | 単調変化しない while ループ -> 変数のインクリメントを追加     |
| ERR_LINEAR_UNCONSUMED_HANDLE       | 獲得した #handle が未消費 -> @send(#h) を全分岐に追加         |
| ERR_LINEAR_DOUBLE_SEND             | 同一ハンドルを 2 回送信 -> 2 回目の @send を削除              |
| ERR_LINEAR_OVERWRITE               | 未消費のハンドル変数へ上書き代入 -> 先に @send を呼ぶ         |
| ERR_MAX_ARRAYS_EXCEEDED            | 配列定義数が上限(8個)を超過 -> 配列定義を統合                 |
| ERR_ARRAY_CAPACITY_EXCEEDED        | 静的配列要素数が上限(256個)を超過 -> 配列サイズを縮小         |
| ERR_UNKNOWN_STRUCT_TYPE            | 未定義の struct 型名を参照 -> struct 定義を確認               |
| ERR_MAX_STRUCTS_EXCEEDED           | 構造体型定義数が上限(8個)を超過 -> 型定義を整理               |
| ERR_MAX_FIELDS_EXCEEDED            | 1 構造体のフィールド数が上限(8個)を超過 -> フィールドを削減   |
| ERR_MAX_TABLES_EXCEEDED            | const テーブル数が上限(8個)を超過 -> テーブルを統合           |
| ERR_PX64_TIMEOUT_EXCEEDED          | 5.0ms TSC Watchdog 超過 -> 処理の分割またはループ段数削減     |
| ERR_PX64_WCET_EXCEEDED             | 10,000ステップ上限超過 -> ループ境界条件を見直し              |
| ERR_PX64_ASSERTION_FAILED          | @assert() の条件式が 0 (偽) -> 事前状態の計算ロジックを検証   |
| ERR_PX64_UNWRAP_FAILED             | @err な Result を @unwrap -> 事前に @is_ok() ガードを設ける   |
| ERR_PX64_ARRAY_OUT_OF_BOUNDS       | 配列インデックスが境界外 -> for 0..N または bounds check を   |
| ERR_PX64_STACK_OVERFLOW            | 関数呼び出し深度が 8 を超過 -> 再帰を排除してループ化         |
+------------------------------------+---------------------------------------------------------------+
```

---

## 8. 低レイヤハードウェアアーキテクチャ & 43 Master Contracts

LatencyOS カーネルおよび `px64` 実行エンジンが厳格に保証する **43 項目の形式アーキテクチャ契約** です。

1. **仕様 WCET 値の完全一致**:
   - 基本命令ディスパッチ: **25 ns** | `@tsc()`: **15 ns** | `@rtt()`: **20 ns** | `@rate()`: **10 ns** | `@capture()`: **100 ns** | `@send()`: **200 ns** | `@print()` / `@println()`: **500 ns** | Glass-to-Glass 総合予算: **8,000 \textmu s (8.00 ms)**
2. **静的 WCET 算出モデル**:
   $$\text{WCET}_{\text{total}} = \sum (\text{Opcode Count} \times 25\text{ ns}) + \sum (\text{Intrinsic WCET})$$
3. **時間型 (Time) のゼロオーバーヘッド即時展開**: コンパイル時に即座にナノ秒整数へ畳み込み。
4. **文字列タグ付きポインタ安全性**: 512B 静的文字列プール内の `offset + len <= 512` を検証。
5. **GPU-to-NIC DMA 完了コヒーレンシ**: `@send(#f)` 時に `sfence` バリアを発行し、`E1000_TXD_STAT_DD` ビットで完了検知。
6. **`!drop` 時のハードウェア記述子自動回収**: `@within` デッドライン超過時、未送信の GPU/NIC DMA スロットを即時フリープールへ返却。
7. **`CALL_NAT` 呼び出し規約**: オペランドで `FuncId` と `ArgReg` を渡し、結果を `$rax` または指定宛先レジスタへ格納。
8. **ネイティブ関数の戻り値規則**: Void 組み込み関数は `0` を返却、値関数は 64-bit 整数またはタグ付き値を返却。
9. **時限ガードの階層スタック (8 レベル)**: ネストされた `@within` は外側よりも短い締切時間（$\text{Deadline}_{\text{inner}} \le \text{Deadline}_{\text{outer}}$）を強制。
10. **`DROP` 命令の発行条件**: 現在の TSC が設定されたデッドライン TSC を超過した場合のみ実行。
11. **アボート時のリソース回収保証**: ステップ上限超過やエラー時、デッドラインスタックをリセットし未消費ハンドルを強制回収。
12. **制御構文の統一ジャンプセマンティクス**: `if-else`、三項式、三項ブロックはすべて `JZ` / `JMP` へ最適化コンパイル。
13. **条件分岐における線形ハンドル検証**: 分岐前に獲得された `#handle` は、**すべての分岐パス（Then および Else）で完全に消費** されなければならない。
14. **ループ内線形ハンドルの反復内消費規則**: ループ内で `@capture()` されたハンドルは、同一反復内で必ず `@send()` されなければならず、ループ外へ脱出できない。
15. **キャプチャ失敗時のセーフガード**: GPU リング枯渇時、`@capture()` は `0` を返却。
16. **送信失敗時の自動回収**: NIC TX リング満杯時、`@send()` はフレームをドロップしてカウンタを加算し、ハンドルを安全に消費済みとしてマーク。
17. **0 除算保護**: `DIV` および `MOD` は除数 `0` に対して `0` を返却（CPU トラップなし）。
18. **64ビット整数オーバーフロー**: 2 の補数演算によるラッピング（`wrapping_add`, `wrapping_sub`, `wrapping_mul`）。
19. **比較演算の真偽値表現**: 真なら `1`、偽なら `0` をレジスタに格納。
20. **内部ブール型のセマンティクス**: `0` は偽、非ゼロはすべて真として評価。
21. **文字列ポインタのメモリ安全性**: ユーザー空間からの任意ポインタ構築を禁止し、プール境界内のみ参照。
22. **静的文字列プール上限**: 合計 512 バイトを超過した場合はコンパイルエラー。
23. **スタックオーバーフロー保護**: コール深度 8 フレーム超過で即時停止。
24. **バイナリフォーマット検証**: `PX64` マジック、バージョン 3、4 バイト命令アライメントの検証。
25. **バイナリバージョニング**: `PX64` ヘッダーの Version は厳格に `0x0003`。
26. **ハードウェアターゲット**: x86_64 CPU (Invariant TSC 必須), Intel 82540EM/82545EM NIC (e1000), 1920x1080@32bpp Linear VRAM。
27. **TSC 単位と周波数**: 1 TSC tick = 1 CPU サイクル。
28. **TSC からナノ秒への高精度変換**:
    $$\text{Time (ns)} = \frac{\text{Ticks} \times 1,000,000,000}{\text{TSC Frequency (Hz)}}$$
29. **C0 ステートロック & 周波数固定**: MSR `0x1A0` / `0x1B0` により全コアを最高周波数 C0 に固定し、熱ジッタを排除。
30. **割り込み隔離 & ISR 実行上限**: Core 1〜3 は `cli`（割り込み完全禁止）。Core 0 のタイマー ISR は $\le 150\text{ ns}$ に制約。
31. **キャッシュミス WCET モデリング**: ホットループの L1/L2 キャッシュヒット（< 4 ns）、DRAM コールドアクセス（$\le 100\text{ ns}$）を前提にモデリング。
32. **DMA キャッシュコヒーレンシ**: DMA 領域は Uncached (UC) または Write-Combining (WC) ページに配置し、`sfence` / `clflush` を適用。
33. **メモリバリア発行条件**: フレーム記述子書込後に `sfence`、SPSC リングポインタ更新時に `mfence` を発行。
34. **4 コアメモリオーダリング**: SPSC ロックフリーリングによる `Acquire` / `Release` メモリ順序付け。
35. **VBLANK イベント排他ポーリング**: Core 1 のみが GPU VBLANK ステータスレジスタを排他ポーリングし、SMP ロック競合をゼロ化。
36. **パイプラインバッファライフサイクル**: Stage 0 (ISR) $\to$ Stage 1 (Userspace) $\to$ Stage 2 (VBLANK) $\to$ Stage 3 (Capture) $\to$ Stage 4 (Encode) $\to$ Stage 5 (Network TX) $\to$ Release。
37. **DMA バッファライフサイクル**: Free $\to$ Capture $\to$ DMA Transfer $\to$ TX Complete $\to$ Free。
38. **NIC TX 完了ポーリング**: Core 3 がハードウェア割り込みなしで `E1000_TXD_STAT_DD` ビットをポーリング。
39. **GPU フレームバッファ再利用**: 次フレームの VBLANK エッジ検知時に旧スロットを安全に解放。
40. **コンパイラ診断エラー復帰**: 単一パスコンパイルで最初のエラー位置を行・列・トークン・バイトオフセット付きで正確に特定。
41. **有界ループ証明の形式性**: `while` ループの条件変数は単調変化が必須。
42. **動的 TSC レイテンシと静的 WCET の二重ガード**: 静的解析による上限保証に加え、実行時 `@within` による実時間監視を重畳。
43. **モジュール空間の静的リンク**: モジュール間呼び出しにおけるオーバーヘッドを定数 25 ns として加算。

---

## 9. 標準スクリプトレシピ集 (`.pul`) 実践カタログ

### 9.1 `stream.pul` (GPU-to-NIC ゼロコピーパイプライン)
```pulse
// stream.pul - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
@pipeline: UltraStream @budget(8000us);

@on_vblank: {
    #f0 := @capture();
    @within(500us) {
        let $rtt = @rtt();
        if ($rtt > 200us) {
            @rate(80);
        } else {
            @rate(100);
        }
        @send(#f0);
    } !drop;
};
```

---

### 9.2 `bench.pul` (実時間演算 & レイテンシベンチマーク)
```pulse
// bench.pul - Realtime Math & Latency Benchmark
@contract: @wcet(5us) @budget(50us);

let $t0 = @tsc();
let mut $sum = 0;

for $i in 0..100 {
    $sum += $i * 2;
}

let $dt = @tsc() - $t0;
@println("[BENCH] Iterations: 100");
@print("[RESULT] Sum: ");
@println($sum);
@print("[LATENCY] Elapsed Cycles: ");
@println($dt);
```

---

### 9.3 `filter.pul` (適応型輻輳制御ガード)
```pulse
// filter.pul - Adaptive Congestion Guard
@contract: @wcet(2us) @budget(100us);

let $rtt = @rtt();
@print("[FILTER] Measured RTT (ns): ");
@println($rtt);

if ($rtt > 300us) {
    @println("[ACTION] Congestion detected -> Rate: 60%");
    @rate(60);
} else {
    @println("[ACTION] Optimal latency -> Rate: 100%");
    @rate(100);
}
```

---

### 9.4 `echo.pul` (コマンドライン引数エコー & 文字列処理)
```pulse
// echo.pul - Command-Line Argument Processing & Echo
@contract: @wcet(2us) @budget(20us);

let $argc = @argc();
if ($argc > 0) {
    let mut $i = 0;
    while ($i < $argc) {
        @print(@arg($i));
        $i += 1;
        if ($i < $argc) {
            @print(" ");
        }
    }
    @println("");
} else {
    @println("LatencyOS PulseLang Real-Time Script Engine Active");
}
```

---

### 9.5 `math_demo.pul` (数学・ビット演算・クランプ・CRC32)
```pulse
// math_demo.pul - Complete Math, Bitwise & Hash Catalog Demo
@contract: @wcet(5us) @budget(50us);

let $val = -42;
let $abs_val = @abs($val);
@assert($abs_val == 42);

let $min_v = @min(100, 200);
let $max_v = @max(100, 200);
@assert($min_v == 100);
@assert($max_v == 200);

let $clamped = @clamp(150, 10, 100);
@assert($clamped == 100);

let $mask = 0b10110010;
let $pop = @popcnt($mask);
@assert($pop == 4);

let $crc = @crc32(0xFFFFFFFF, 12345678);
@print("[MATH] CRC32 Checksum: ");
@println($crc);
```

---

### 9.6 `telemetry_ext.pul` (拡張ハードウェアテレメトリ)
```pulse
// telemetry_ext.pul - Comprehensive Multi-Core Hardware Telemetry
@contract: @wcet(3us) @budget(30us);

let $core = @core_id();
let $freq = @tsc_freq();
let $uptime = @uptime_ns();
let $q_cap = @ring_depth(0);
let $q_net = @ring_depth(1);

@println("=== LatencyOS Extended Hardware Telemetry ===");
@print("[CORE] Current CPU Core APIC ID: ");
@println($core);
@print("[FREQ] Invariant TSC Frequency (MHz): ");
@println($freq);
@print("[TIME] System Uptime (ns): ");
@println($uptime);
@print("[RING] Capture-to-Encode Queue Depth: ");
@println($q_cap);
@print("[RING] Encode-to-Net Queue Depth: ");
@println($q_net);
```

---

### 9.7 `vram_test.pul` (GPU フレームバッファ直接操作)
```pulse
// vram_test.pul - Direct Framebuffer VRAM Read & Write Test
@contract: @wcet(10us) @budget(100us);

let $slot = 0;
let $offset = 2048;
let $pixel_color = 0x00FF00FF; // ARGB Magenta

// 1. VRAM へのピクセル書き込み
@vram_write($slot, $offset, $pixel_color);

// 2. VRAM からの読み出し検証
let $read_back = @vram_read($slot, $offset);
@assert($read_back == $pixel_color);

@println("[VRAM] Framebuffer Slot 0 Read/Write verified successfully.");
```

---

### 9.8 `fn_test.pul` (静的関数定義 & コールスタック検証)
```pulse
// fn_test.pul - Static Function Declaration & Call Stack Verification
@contract: @wcet(4us) @budget(40us);

fn multiply_and_offset($base, $factor, $offset) {
    let $product = $base * $factor;
    let $result = $product + $offset;
    return $result;
}

let $calculated = multiply_and_offset(10, 5, 25);
@assert($calculated == 75);
@print("[FN] Function returned expected value: ");
@println($calculated);
```

---

### 9.9 `struct_test.pul` (静的構造体定義 & フィールドアクセス)
```pulse
// struct_test.pul - Static Struct Definition, Instantiation & Update
@contract: @wcet(4us) @budget(40us);

struct NetworkPacketHeader {
    magic: i64,
    sequence: i64,
    payload_len: i64,
    checksum: i64,
};

let mut $pkt: NetworkPacketHeader;

$pkt.magic = 0x50583634; // "PX64"
$pkt.sequence = 101;
$pkt.payload_len = 1400;
$pkt.checksum = @crc32(0, $pkt.sequence);

@assert($pkt.magic == 0x50583634);
@assert($pkt.sequence == 101);
@assert($pkt.payload_len == 1400);

@println("[STRUCT] Network packet header fields initialized and validated.");
```

---

### 9.10 `match_test.pul` (Tagged Result パターンマッチング)
```pulse
// match_test.pul - Tagged Result Ok/Err Handling & Pattern Matching
@contract: @wcet(3us) @budget(30us);

fn validate_latency($current_rtt_ns) {
    if ($current_rtt_ns <= 500000) { // 500us
        return @ok($current_rtt_ns);
    } else {
        return @err(504); // Gateway Timeout / High Latency
    }
}

let $status = validate_latency(250000); // 250us -> Ok

match $status {
    Ok($latency) => {
        @print("[RESULT_OK] Latency within budget: ");
        @println($latency);
    },
    Err($err_code) => {
        @print("[RESULT_ERR] Latency violation error: ");
        @println($err_code);
    },
    _ => {
        @println("[RESULT_UNKNOWN] Unhandled state");
    },
};
```

---

> **ドキュメント保守情報**:  
> 本リファレンスは `pulselang-core` および `LatencyOS` カーネルの最新実装と完全に同期しています。  
> 新たな Intrinsics や Opcode が追加された場合は、本仕様書の対応するカタログ表・EBNF・契約定義を更新してください。
