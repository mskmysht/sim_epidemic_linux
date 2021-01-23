# sim_epidemic_linux

本プログラムは https://github.com/unemi/SimEpidemic の一部の機能をLinux環境で動作するように移植したものである．

### 概要
- ブランチ`App-1.7-+-Server-1.2`を参考に実装
- ネットワーク通信，シナリオ機能を除いた，世界の生成と実行機能が実装
- パラメータは世界(`World`)の生成時に`WorldParams`構造体と`RuntimeParams`構造体を用いて設定