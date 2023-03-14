# SimEpidemic for Linux

本リポジトリは[個体ベース感染シミュレータ](http://www.intlab.soka.ac.jp/~unemi/SimEpidemic1/info/)プロジェクトの一部で、
Linux HPC向けのシミュレーションジョブの管理システムです。

## アプリケーション構成
主要なアプリケーションは以下のとおりです。

|アプリケーション|説明|
|-|-|
|controller|ジョブ実行用REST APIアプリケーション|
|worker|シミュレータ管理アプリケーション|
|world|感染シミュレータプログラム|
|cert-gen|ルート証明書生成用プログラム|

## システム構成
- controllerサーバ x 1台
- workerサーバ x N台

## セットアップ
### workerサーバ
1. ソースコードのビルド

   1. Rustの[インストール](https://www.rust-lang.org/ja/tools/install)

   1. リポジトリのダウンロード

   1. リポジトリ直下にてビルドコマンドの実行
      ```console
      $ cargo build --release --package worker --package world
      ```

1. 統計情報を保存するディレクトリの作成

1. サーバ証明書の準備

   同梱の`cert-gen`でルート証明書と秘密鍵を生成できます。
   ```console
   $ cargo build --release --package cert-gen
   ```
   生成例（ドメイン名：`worker1`）
   ```console
   $ ./target/release/cert-gen worker1
   ```
   `cert-gen`の詳細は`--help`を参照してください。

1. 設定ファイルの準備

   以下の項目からなるTOMLファイルを作成します。
   ```toml
   cert_path = "./ca_cert.der"           # 証明書のパス
   pkey_path = "./ca_pkey.der"           # 秘密鍵のパス
   world_path = "./target/release/world" # `world`プログラムのパス
   addr = "192.168.1.11:3000"            # workerサーバのIPアドレス+ポート番号
   max_population_size = 20000           # サーバが許容する最大の人口
   max_resource = 100                    # 最大リソース量（リソースの分解能）
   stat_dir = "./dump"                   # 統計情報の保存先
   ```
   #### 補足
   - リソースはサーバの使用率を表すための量で、単位リソース量は１タスクの最大計算コストを`max_resource`で割った値になります。
   タスクの計算コストは`world`の人口（`population_size`）の２乗で計算されます。

   - タスクの計算コストが単位リソース量よりも小さい場合、単位リソース分で換算されます。

   - タスクの計算コストが最大リソース量よりも大きい場合、タスクは拒否されます。

### controllerサーバ
1. ソースコードのビルド

   1. Rustの[インストール](https://www.rust-lang.org/ja/tools/install)

   1. リポジトリのダウンロード

   1. リポジトリ直下にてビルドコマンドの実行
      ```console
      $ cargo build --release --package controller
      ```

1. データベースのセットアップ

   - PostgreSQLの[インストール](https://www.rust-lang.org/ja/tools/install)
   
     必要に応じてユーザの追加、パスワードの設定をします。

   - 初期化スクリプトの実行
      ```console
      $ psql -U [username] -f script/init_table.sql
      ```

1. 設定ファイルの準備

   以下の項目からなるTOMLファイルを作成します。
   ```toml
   addr = "192.168.1.10"   # controllerサーバのIPアドレス
   port = 8080             # REST APIのListenポート番号
   db_username = "simepi"  # PostgreSQLのユーザ名
   db_password = "simepi"  # PostgreSQLのパスワード
   max_job_request = 127   # ジョブの最大リクエスト数

   # workerサーバの設定
   #  - controller_port  workerごとのcontrollerサーバのポート番号
   #  - cert_path        workerの証明書
   #  - addr             workerのIPアドレス+ポート番号
   #  - domain           証明書に登録されているworkerのドメイン名
   workers = [
      { controller_port = 3000, cert_path = "ca_cert1.der", addr = "192.168.1.11:3000", domain = "worker1" },
      { controller_port = 3001, cert_path = "ca_cert2.der", addr = "192.168.1.12:3000", domain = "worker2" },
   ]
   ```

## システムの起動
- workerサーバ
   ```console
   $ ./target/release/worker [設定ファイルのパス]
   ```

- controllerサーバ
   ```console
   $ ./target/release/controller [設定ファイルのパス]
   ```

## ジョブの実行
REST APIのドキュメント（`http://[controllerサーバのアドレス]/doc`）を参照してください。

&copy; Masaaki Miyashita and Tatsuo Unemi, 2020-2023, All rights reserved.
