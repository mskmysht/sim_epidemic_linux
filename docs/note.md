### World.m
- `deliverTestResults`
    - チェック後に検査対象を追加している
    - 検査実施は抽出時から１ステップ分遅れる
    - 最終ステップで抽出された対象は検査が行われない

### 全般
- `TracingOperation`
    - bitflags[https://github.com/bitflags/bitflags]で代用