# 飞行棋游戏设计图（按需求与规则）

本文档基于以下两份文档生成：

- [requirement.md](/Users/zhaodaojun/Documents/studio/404man/code/aeroplane-chess/requirement.md)
- [rules-spec.md](/Users/zhaodaojun/Documents/studio/404man/code/aeroplane-chess/docs/rules-spec.md)

## 1. 整体玩法流程图

```mermaid
flowchart TD
    A[启动游戏] --> B[主菜单]
    B --> C[模式与开局配置]
    C --> C1[选择模式: 1v1 / 2v2]
    C --> C2[选择人类颜色]
    C --> C3[选择每人棋子数 1~4]
    C1 --> D[进入对局]
    C2 --> D
    C3 --> D
    D --> E[回合循环]
    E --> F[投骰]
    F --> G{有合法行动?}
    G -- 否 --> H[结束本次行动/保留额外投骰规则]
    G -- 是 --> I[选择棋子并移动]
    I --> J[结算格子效果/技能/撞击/叠加]
    J --> K{满足胜利条件?}
    K -- 否 --> L{是否额外投骰?}
    L -- 是 --> F
    L -- 否 --> M[切换下一个玩家]
    M --> E
    K -- 是 --> N[结果结算页]
```

## 2. AppState 状态机图

```mermaid
stateDiagram-v2
    [*] --> Boot
    Boot --> MainMenu
    MainMenu --> ModeSelect
    ModeSelect --> MainMenu: Esc
    ModeSelect --> LoadingGame: Enter
    LoadingGame --> InGame
    InGame --> Result: 胜负已判定
    Result --> MainMenu: 再来一局/返回菜单
```

## 3. 对局内 GamePhase 状态机图

```mermaid
stateDiagram-v2
    [*] --> RoundStart
    RoundStart --> AwaitDice
    AwaitDice --> DiceRolling
    DiceRolling --> AwaitPieceSelect: 有可行动棋子
    DiceRolling --> RoundEnd: 无可行动棋子
    AwaitPieceSelect --> PieceMoving
    PieceMoving --> ResolveTileEffect
    ResolveTileEffect --> ResolveSkillEffect
    ResolveSkillEffect --> ResolveCombat
    ResolveCombat --> CheckVictory
    CheckVictory --> RoundEnd: 未结束
    CheckVictory --> [*]: 对局结束
    RoundEnd --> AwaitDice
```

## 4. 开局配置交互图（新增规则）

```mermaid
flowchart LR
    A[ModeSelect 页面] --> B[按 1/2 切换模式]
    A --> C[按 C 切换人类颜色]
    A --> D[按 [ 或 - 减少棋子数]
    A --> E[按 ] 或 = 增加棋子数]
    D --> F[棋子数限制到 1..4]
    E --> F
    B --> G[实时刷新 HUD 文本]
    C --> G
    F --> G
    G --> H[按 Enter 开始]
    H --> I[生成 MatchConfig + 玩家队伍与棋子]
```

## 5. 回合结算时序图（规则执行）

```mermaid
sequenceDiagram
    participant P as 当前玩家
    participant T as TurnFlow
    participant S as SkillFlow
    participant R as RuleResolver
    participant V as VictoryCheck

    P->>T: 投骰
    T->>S: 解析是否双骰/冲刺等
    S-->>T: 最终点数与加成
    T->>R: 选择行动(起飞/移动)
    R-->>T: 位置变更
    T->>R: 结算飞跃/事件/护盾/撞击/叠加
    R-->>T: 本次行动结果与日志
    T->>V: 计算每位玩家是否全棋子完成
    V-->>T: 胜负状态
    alt 对局结束
        T-->>P: 进入结果页
    else 未结束
        T-->>P: 切换到下一玩家回合
    end
```

## 6. 核心数据关系图

```mermaid
classDiagram
    class MatchSetup {
      +mode
      +ai_difficulty
      +fast_mode
      +human_color
      +pieces_per_player
    }

    class MatchConfig {
      +mode
      +ai_difficulty
      +fast_mode
      +human_color
      +pieces_per_player
    }

    class PlayerRoster {
      +players: Vec~PlayerProfile~
    }

    class PlayerProfile {
      +state: PlayerState
      +color
      +hangar_slots
      +launch_tile_index
      +home_lane_positions
      +goal_position
    }

    class TeamRoster {
      +teams: Vec~TeamState~
    }

    class PieceState {
      +owner_player_id
      +team_id
      +status(InHangar/Active/Finished)
      +progress
      +shield
      +stack_shield
    }

    MatchSetup --> MatchConfig : 初始化复制
    MatchConfig --> PlayerRoster : 构建玩家
    MatchConfig --> TeamRoster : 构建队伍
    PlayerRoster --> PlayerProfile
    TeamRoster --> PieceState : 用于胜负判定聚合
```

## 7. HUD 布局线框图（避免遮挡版）

```mermaid
flowchart TB
    Root[Game Root]
    Root --> TopBar[顶部状态栏: 模式/回合/阶段/最后骰子]
    Root --> Board[棋盘主区域]
    Root --> Side[侧边可折叠 HUD]
    Side --> SkillPanel[技能区: Dash/Snipe/Swap/Shield/DoubleDice]
    Side --> ActionLog[行动日志]
    Side --> Prompt[提示区: 空格投骰/选棋子]
    Side --> FoldBtn[折叠按钮]
```

---

以上设计图覆盖了当前 MVP 已实现与近期规则改造（开局颜色、棋子数量 1~4）对应的核心交互与数据流，可直接用于后续 UI 重构、功能补完与测试用例映射。
