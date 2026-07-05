use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::data::rule_set::RuleSet;
use crate::domain::rules::LaunchRule;
use crate::gameplay::ai::AiDifficulty;
use crate::plugins::skill_plugin::SkillUiAction;

/// 当前内置的语言资源；后续增加语言时扩展 `Language` 和对应文案表即可。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    English,
    SimplifiedChinese,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::SimplifiedChinese, Self::English];

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|language| *language == self)
            .unwrap_or_default();
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct LanguageSettings {
    pub language: Language,
}

impl LanguageSettings {
    pub fn cycle(&mut self) {
        self.language = self.language.next();
    }

    pub fn label(self) -> &'static str {
        self.language.label()
    }
}

impl Default for LanguageSettings {
    fn default() -> Self {
        Self {
            language: Language::SimplifiedChinese,
        }
    }
}

/// 多语言基础插件：注册语言状态，并给所有 Bevy 文本使用内置中文字体。
pub struct I18nPlugin;

impl Plugin for I18nPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LanguageSettings>()
            .init_resource::<I18nFont>()
            .add_systems(Startup, load_i18n_font)
            .add_systems(Update, (apply_i18n_font, update_localized_text));
    }
}

#[derive(Debug, Default, Resource)]
struct I18nFont {
    font: Option<Handle<Font>>,
}

const CJK_FONT_PATH: &str = "fonts/NotoSansCJKsc-Regular.otf";

fn load_i18n_font(asset_server: Res<AssetServer>, mut i18n_font: ResMut<I18nFont>) {
    i18n_font.font = Some(asset_server.load(CJK_FONT_PATH));
}

fn apply_i18n_font(
    i18n_font: Res<I18nFont>,
    mut font_query: Query<&mut TextFont, Or<(Added<TextFont>, Changed<TextFont>)>>,
) {
    let Some(font) = i18n_font.font.as_ref() else {
        return;
    };
    let font_source = FontSource::Handle(font.clone());

    for mut text_font in &mut font_query {
        if text_font.font != font_source {
            text_font.font = font_source.clone();
        }
    }
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct LocalizedText {
    pub key: TextKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextKey {
    Settings,
    Audio,
    Music,
    Effects,
    Mute,
    FpsCounter,
    Language,
    MainMenu,
    QuitGame,
    GameTitle,
    StartMatch,
    SoundSettings,
    BackgroundMusic,
    ActionEffects,
    Back,
    Mode,
    PlayStyle,
    PiecesPerPlayer,
    LaunchRule,
    AiDifficulty,
    Start,
    Human,
    Ai,
    EventLog,
    EventLogOpen,
    EventLogClosed,
    MatchStarted,
    SkillLocked,
    SkillTipClose,
    ResultTitle,
    RestartMatch,
}

pub fn text(language: Language, key: TextKey) -> &'static str {
    match language {
        Language::SimplifiedChinese => match key {
            TextKey::Settings => "设置",
            TextKey::Audio => "声音",
            TextKey::Music => "音乐",
            TextKey::Effects => "音效",
            TextKey::Mute => "静音",
            TextKey::FpsCounter => "帧率显示",
            TextKey::Language => "语言",
            TextKey::MainMenu => "主菜单",
            TextKey::QuitGame => "退出游戏",
            TextKey::GameTitle => "飞行棋",
            TextKey::StartMatch => "开始对局",
            TextKey::SoundSettings => "声音设置",
            TextKey::BackgroundMusic => "背景音乐",
            TextKey::ActionEffects => "操作音效",
            TextKey::Back => "返回",
            TextKey::Mode => "模式",
            TextKey::PlayStyle => "玩法",
            TextKey::PiecesPerPlayer => "每人棋子",
            TextKey::LaunchRule => "起飞规则",
            TextKey::AiDifficulty => "电脑难度",
            TextKey::Start => "开始",
            TextKey::Human => "真人",
            TextKey::Ai => "电脑",
            TextKey::EventLog => "日志",
            TextKey::EventLogOpen => "日志 -",
            TextKey::EventLogClosed => "日志 +",
            TextKey::MatchStarted => "对局开始",
            TextKey::SkillLocked => "禁用",
            TextKey::SkillTipClose => "×",
            TextKey::ResultTitle => "对局结果",
            TextKey::RestartMatch => "再来一局",
        },
        Language::English => match key {
            TextKey::Settings => "Settings",
            TextKey::Audio => "Audio",
            TextKey::Music => "Music",
            TextKey::Effects => "Effects",
            TextKey::Mute => "Mute",
            TextKey::FpsCounter => "FPS Counter",
            TextKey::Language => "Language",
            TextKey::MainMenu => "Main Menu",
            TextKey::QuitGame => "Quit Game",
            TextKey::GameTitle => "Aeroplane Chess",
            TextKey::StartMatch => "Start Match",
            TextKey::SoundSettings => "Sound Settings",
            TextKey::BackgroundMusic => "Background Music",
            TextKey::ActionEffects => "Action Effects",
            TextKey::Back => "Back",
            TextKey::Mode => "Mode",
            TextKey::PlayStyle => "Play Style",
            TextKey::PiecesPerPlayer => "Pieces / Player",
            TextKey::LaunchRule => "Launch Rule",
            TextKey::AiDifficulty => "AI Difficulty",
            TextKey::Start => "Start",
            TextKey::Human => "Human",
            TextKey::Ai => "AI",
            TextKey::EventLog => "Log",
            TextKey::EventLogOpen => "Log -",
            TextKey::EventLogClosed => "Log +",
            TextKey::MatchStarted => "Match started",
            TextKey::SkillLocked => "LOCK",
            TextKey::SkillTipClose => "x",
            TextKey::ResultTitle => "Match Result",
            TextKey::RestartMatch => "Restart Match",
        },
    }
}

fn update_localized_text(
    language_settings: Res<LanguageSettings>,
    mut text_query: Query<(&LocalizedText, &mut Text)>,
) {
    if !language_settings.is_changed() {
        return;
    }

    for (localized_text, mut text_component) in &mut text_query {
        *text_component = Text::new(text(language_settings.language, localized_text.key));
    }
}

pub fn mode_label(language: Language, mode: GameMode) -> &'static str {
    match (language, mode) {
        (_, GameMode::OneVsOne) => "1v1",
        (_, GameMode::TwoVsTwo) => "2v2",
        (Language::SimplifiedChinese, GameMode::FreeForAll) => "混战",
        (Language::English, GameMode::FreeForAll) => "FFA",
    }
}

pub fn rule_set_label(language: Language, rule_set: RuleSet) -> &'static str {
    match (language, rule_set) {
        (Language::SimplifiedChinese, RuleSet::Traditional) => "传统",
        (Language::SimplifiedChinese, RuleSet::Creative) => "创意",
        (Language::English, RuleSet::Traditional) => "Traditional",
        (Language::English, RuleSet::Creative) => "Creative",
    }
}

pub fn launch_rule_label(language: Language, launch_rule: LaunchRule) -> &'static str {
    match (language, launch_rule) {
        (Language::SimplifiedChinese, LaunchRule::Even) => "偶数",
        (Language::SimplifiedChinese, LaunchRule::SixOnly) => "仅6点",
        (Language::English, LaunchRule::Even) => "2/4/6",
        (Language::English, LaunchRule::SixOnly) => "6 only",
    }
}

pub fn ai_difficulty_label(language: Language, difficulty: AiDifficulty) -> &'static str {
    match (language, difficulty) {
        (Language::SimplifiedChinese, AiDifficulty::Easy) => "简单",
        (Language::SimplifiedChinese, AiDifficulty::Normal) => "普通",
        (Language::SimplifiedChinese, AiDifficulty::Hard) => "困难",
        (Language::English, AiDifficulty::Easy) => "Easy",
        (Language::English, AiDifficulty::Normal) => "Normal",
        (Language::English, AiDifficulty::Hard) => "Hard",
    }
}

pub fn skill_name(language: Language, action: SkillUiAction) -> &'static str {
    match language {
        Language::SimplifiedChinese => skill_token(language, skill_key(action)),
        Language::English => skill_token(language, skill_key(action)),
    }
}

pub fn skill_tip_body(language: Language, action: SkillUiAction) -> &'static str {
    match language {
        Language::SimplifiedChinese => match action {
            SkillUiAction::Dash => "掷骰后移动 +3，再选择一架可移动飞机。",
            SkillUiAction::Snipe => "攻击公用航道或冲刺道上的敌方飞机，护盾会优先抵挡。",
            SkillUiAction::Swap => {
                "与一架合法主航道目标飞机交换位置；2v2 目标为队友，其他模式目标为敌机。"
            }
            SkillUiAction::Shield => "给己方一架已起飞飞机加 1 层护盾，最多 2 层。",
            SkillUiAction::DoubleDice => "下一次掷两个骰子，并选择其中一个作为本回合点数。",
        },
        Language::English => match action {
            SkillUiAction::Dash => {
                "After rolling, add +3 movement before choosing one movable aircraft."
            }
            SkillUiAction::Snipe => {
                "Target an enemy aircraft on the public or home lane. Shields absorb the hit first."
            }
            SkillUiAction::Swap => {
                "Swap with a legal main-route target: teammate in 2v2, enemy otherwise."
            }
            SkillUiAction::Shield => {
                "Give one active friendly aircraft a shield, up to 2 layers. Shields block hits."
            }
            SkillUiAction::DoubleDice => "Arm the next roll with two dice, then choose one result.",
        },
    }
}

fn skill_key(action: SkillUiAction) -> &'static str {
    match action {
        SkillUiAction::Dash => "Dash",
        SkillUiAction::Snipe => "Snipe",
        SkillUiAction::Swap => "Swap",
        SkillUiAction::Shield => "Shield",
        SkillUiAction::DoubleDice => "DoubleDice",
    }
}

pub fn skill_token(language: Language, skill: &str) -> &'static str {
    match language {
        Language::SimplifiedChinese => match skill {
            "Dash" => "冲刺",
            "Snipe" => "狙击",
            "Swap" => "换位",
            "Shield" => "护盾",
            "DoubleDice" => "双骰",
            _ => "技能",
        },
        Language::English => match skill {
            "Dash" => "Dash",
            "Snipe" => "Snipe",
            "Swap" => "Swap",
            "Shield" => "Shield",
            "DoubleDice" => "DoubleDice",
            _ => "Skill",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_simplified_chinese() {
        let settings = LanguageSettings::default();

        assert_eq!(settings.language, Language::SimplifiedChinese);
        assert_eq!(settings.label(), "简体中文");
    }

    #[test]
    fn language_cycle_switches_between_chinese_and_english() {
        let mut settings = LanguageSettings::default();

        settings.cycle();
        assert_eq!(settings.language, Language::English);
        assert_eq!(settings.label(), "English");

        settings.cycle();
        assert_eq!(settings.language, Language::SimplifiedChinese);
    }

    #[test]
    fn text_table_covers_shared_labels() {
        assert_eq!(text(Language::SimplifiedChinese, TextKey::Settings), "设置");
        assert_eq!(text(Language::English, TextKey::Settings), "Settings");
        assert_eq!(
            skill_name(Language::SimplifiedChinese, SkillUiAction::Dash),
            "冲刺"
        );
        assert_eq!(skill_name(Language::English, SkillUiAction::Dash), "Dash");
    }
}
