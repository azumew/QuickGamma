# QuickGamma

QuickGamma は、Windows 用の軽量なガンマ補正ツールです。
タスクトレイに常駐し、各モニターのガンマ値を個別に、または一括で素早く調整することができます。

## 主な機能

- **マルチモニター対応**: 接続されているモニターを個別に認識し、それぞれ異なるガンマ値を設定できます。
- **タスクトレイ常駐**: ウィンドウを最小化または閉じるとタスクトレイに常駐し、いつでも呼び出せます。
- **最小化ボタン**: ウィンドウ右上の最小化ボタンでタスクバーに最小化できます。
- **ショートカット作成**: 現在のガンマ設定を適用するショートカットを作成し、ワンクリックで設定を復元できます。
- **Mica デザイン**: Windows 11 のモダンな UI デザイン（Mica マテリアル）に対応しています（対応環境のみ）。

## 使い方

1. **起動**: `QuickGamma.exe` を起動すると、タスクトレイにアイコンが表示されます。
2. **メイン画面の表示**: タスクトレイアイコンをクリックするか、アイコンを右クリックして「Open」を選択します。
3. **ガンマ値の調整**:
    - **Monitor**: 調整したいモニターを選択します。
    - **Slider**: スライダーを動かすか、数値を入力してガンマ値を調整します（デフォルトは 1.00）。
4. **設定の適用**:
    - **OK**: 設定を適用してウィンドウを隠します（タスクトレイには残ります）。
    - **最小化**: タスクバーに最小化します。
    - **Cancel**: 変更を破棄して、ウィンドウを開く前の状態に戻します。
5. **その他**:
    - **Default All**: 全てのモニターのガンマ値を 1.00 にリセットします。
    - **Make Shortcut**: 現在の全モニターの設定値を適用するコマンドライン引数付きのショートカットをデスクトップに作成します。

## 動作環境

- OS: Windows 10 / 11
- 必須ランタイム: 特になし（標準の Windows API のみ使用）

## ビルド方法

開発環境として Visual Studio (2019/2022) の C++ デスクトップ開発環境が必要です。
`cl.exe` (Microsoft C++ Compiler) を使用してビルドします。

**Developer Command Prompt for VS** を開き、プロジェクトのルートディレクトリで以下のコマンドを実行してください。

```cmd
rc src/quickgamma.rc
cl /DUNICODE /D_UNICODE /EHsc /std:c++17 /Fe:QuickGamma.exe src/main.cpp src/gamma_ctrl.cpp src/shortcut.cpp src/quickgamma.res user32.lib gdi32.lib shell32.lib ole32.lib comctl32.lib dwmapi.lib uxtheme.lib shlwapi.lib /link /SUBSYSTEM:WINDOWS
```
