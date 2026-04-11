use bevy::prelude::*;

use crate::states::AppState;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_menu_placeholder);
    }
}

fn spawn_menu_placeholder(mut commands: Commands) {
    commands.spawn(Name::new("MainMenuRoot"));
}
