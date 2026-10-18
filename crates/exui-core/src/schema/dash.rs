//! Classes registered by the dashboard itself. The dash crate registers
//! these after the XUI library groups.
//!
//! Generated from the dash 6717 class registration functions; addresses are
//! the registering functions. `MediaSourceSelectionScene` is registered by
//! three functions with identical schema; only the first call succeeds at
//! runtime, so it appears once.

use crate::error::RegisterError;
use crate::schema::ClassDef;
use crate::schema::DefaultValue;
use crate::schema::PropDef;
use crate::schema::PropFlags;
use crate::schema::PropType;
use crate::schema::Registry;

/// Dashboard classes ordered so that every base precedes its derived classes.
pub static CLASSES: [&ClassDef; 313] = [
    &LIVE_VISION_CONTROL,
    &LIVE_VISION_DASH_EFFECT,
    &DASH_MAIN_SCENE,
    &DVD_TRAY_SCENE,
    &TRAY_SCENE_BUTTON,
    &TRAY_SCENE_LOADER,
    &C_TITLE_META_DATA_SCENE,
    &C_XUI_EDIT_AUTO_SCROLL,
    &MEDIA_PACKAGE,
    &C_LIVE_MESSAGE_BOX,
    &META_PANEL_SCENE,
    &DASH_SCENE,
    &C_WAIT_FOR_LOGON_SCENE,
    &NETWORK_DEVICE_SELECT_SCENE,
    &NETWORK_DEVICE_META_PANEL_SCENE,
    &NETWORK_DEVICE_MANAGEMENT_SCENE,
    &DASH_SYSTEM_SCENE,
    &WALLPAPER,
    &DASH_SIGN_OUT,
    &C_DASH_LIST_CHOOSER,
    &MEDIA_SOURCE_NO_COMPUTERS_METAPANEL_SCENE,
    &MEDIA_SOURCE_LIST,
    &DASH_VIDEO,
    &C_BLADE_MOTD,
    &DASH_BLADE_TAB,
    &DASH_SETTINGS_ROOT,
    &DASH_C_VALUE_SPIN,
    &DASH_C_DATE,
    &DASH_C_TIMEZONE,
    &DASH_C_LANGUAGE,
    &DASH_C_COUNTRY,
    &DASH_C_RATING_VIEW,
    &DASH_C_SCREEN_SIZE,
    &DASH_C_HINTS,
    &DASH_REMOTE_C_LIST,
    &DASH_SETTINGS_LIST,
    &DASH_P_CONTROL_SELECT,
    &DASH_P_CONTROL_SETTINGS_VIDEO,
    &DASH_P_CONTROL_SETTINGS_VIDEO_MOVIE,
    &DASH_P_CONTROL_SETTINGS_VIDEO_TV,
    &DASH_P_CONTROL_SETTINGS_VIDEO_UNRATED,
    &DASH_P_CONTROL_SETTINGS_LIVE_A,
    &DASH_P_CONTROL_SETTINGS_CONTENT,
    &DASH_P_CONTROL_SETTINGS_LIVE_C,
    &DASH_P_CONTROL_SETTINGS_PASSCODE,
    &DASH_P_CONTROL_SETTINGS_PASSCODE_HINT,
    &DASH_PASSCODE_EDIT,
    &DASH_PASSCODE_N_EDIT,
    &C_FAMILY_TIMER_DURATION_LIST,
    &C_FAMILY_TIMER_SETTINGS_SCENE,
    &DASH_C_TEXT_SPIN,
    &DASH_C_TIME,
    &DASH_CC_INPUT_SCENE,
    &DASH_VIDEO_SETTINGS,
    &DASH_VIDEO_SETTINGS_HD,
    &DASH_VIDEO_SETTINGS_FORMAT,
    &DASH_VIDEO_SETTINGS_60,
    &DASH_AUDIO_SETTINGS_D,
    &DASH_AUDIO_SETTINGS_FX,
    &DASH_CLOCK_SETTINGS_MENU,
    &DASH_CLOCK_SETTINGS,
    &DASH_CLOCK_SETTINGS_T_ZONE,
    &DASH_CLOCK_SETTINGS_1224,
    &DASH_COUNTRY_SETTINGS,
    &DASH_AUTO_OFF,
    &DASH_REMOTE_C,
    &DASH_SCREENSAVER,
    &DASH_OUTPUT_LEVELS,
    &DASH_SYSTEM_RESET,
    &DASH_START_UP,
    &DASH_LIVE_VISION_SETTINGS_SCENE,
    &DASH_BACKGROUND_DOWNLOADS,
    &DASH_SHUTDOWN_SETTINGS,
    &DASH_P_CONTROL_SETTINGS_MENU,
    &DASH_P_CONTROL_SETTINGS_GAME,
    &DASH_P_CONTROL_SETTINGS_VIDEO_EXPLICIT,
    &DASH_AUDIO_SETTINGS,
    &DASH_LANGUAGE_SETTINGS,
    &DASH_CLOCK_SETTINGS_D_SAVING,
    &MEDIA_SOURCE_SELECTION_SCENE,
    &GRID_LIST,
    &TRANSPORT_CONTROLS_SCENE,
    &IMAGE_RENDER_CONTROL,
    &DEVICE_MAIN_SCENE,
    &FULL_SCREEN_PHOTO_SCENE,
    &SONG_LIST,
    &MEDIA_CONTAINER_LIST,
    &MUSIC_PLAY_PAUSE_BUTTON,
    &GOTO_MUSIC_PLAYER_SCENE,
    &GOTO_MUSIC_PLAYER_DASH_SCENE,
    &SONG_LIST_SCENE,
    &MEDIA_CONTAINER_LIST_SCENE,
    &MEDIA_CONTAINER_OPTIONS_SCENE,
    &ARTIST_OPTIONS_SCENE,
    &SONG_OPTIONS_SCENE,
    &MUSIC_LIBRARY_SCENE,
    &EDIT_NOW_PLAYING_QUEUE_SCENE,
    &EDIT_NOW_PLAYING_QUEUE_LIST,
    &RIP_CD_PROGRESS_LIST,
    &EDIT_BOX_BUTTON,
    &NOW_PLAYING_SCENE,
    &MUSIC_EQUALIZER,
    &NOW_PLAYING_LIST,
    &FILE_VIEW_SCENE,
    &FILE_VIEW_LIST,
    &MUSIC_PLAYBACK_MODE_BUTTON,
    &DEVICE_NOT_BROWSEABLE_SCENE,
    &SONG_METADATA_SCENE,
    &RIP_CD_PROGRESS_SCENE,
    &RIP_CD_SELECT_SCENE,
    &EDIT_TAG_INFO_SCENE,
    &NOW_PLAYING_RIP_OR_EDIT_BUTTON,
    &VISUALIZER_BUTTON,
    &VISUALIZER_SCENE,
    &MUSIC_TRANSPORT_CONTROLS_SCENE,
    &VIDEO_SCENE,
    &C_POSTAL_DATA_LIST,
    &C_ADDRESS_BUTTON,
    &C_STATE_LIST,
    &C_CONTACT_ADDRESS_SCENE,
    &C_CHANGE_LIVE_ID_CURRENT_SIGN_IN,
    &C_CHANGE_LIVE_ID_NEW_SIGN_IN,
    &C_CHANGE_LIVE_ID_PARENT_SIGN_IN,
    &C_PRIV_TWO_OPTIONS_SCENE,
    &C_PRIV_THREE_OPTIONS_SCENE,
    &C_LIVE_KEYBOARD,
    &CAM_HOME_SCENE,
    &CAM_MY_MEMBERSHIPS_SCENE,
    &C_MY_MEMBERSHIPS_LIST,
    &C_MY_MEMBERSHIPS_SCENE,
    &C_PASSPORT_SIGN_IN_BUTTON,
    &C_PASSPORT_SIGN_IN_SCENE,
    &C_PASSPORT_PASSWORD_BUTTON,
    &C_PASSPORT_PASSWORD_SCENE,
    &CWLID_MENU_SCENE,
    &C_CHANGE_LIVE_ID_MENU_SCENE,
    &C_CHANGE_LIVE_ID_FINISH_SCENE,
    &C_CHANGE_LIVE_ID_CONGRATS_SCENE,
    &PRIVACY_SETTINGS_LIST,
    &C_PRIVACY_MAIN_SCENE,
    &C_MARKETING_TWO_OPTIONS_SCENE,
    &C_POLICY_MAIN_SCENE,
    &C_POLICY_DISPLAY_SCENE,
    &CPC_MAIN_SCENE,
    &CPC_PRIV_FRIENDS_SCENE,
    &C_GAMEPLAY_SETTINGS_SCENE,
    &CPC_CONTENT_SETTINGS_SCENE,
    &C_MORE_INFO_SCENE,
    &C_ABOUT_MS_POINTS_LIST_SCENE,
    &C_POSTAL_DATA_LIST_SCENE,
    &C_STATE_LIST_SCENE,
    &C_MESSENGER_WELCOME_SCENE,
    &C_MESSENGER_SELECT_WLID_SCENE,
    &C_MESSENGER_SIGNUP_COMPLETE_SCENE,
    &C_FRIEND_REQUEST_RECIPIENTS_LIST,
    &C_FRIEND_REQUEST_INTRO_SCENE,
    &C_FRIEND_REQUEST_MAIN_SCENE,
    &FRIEND_REQUEST_PROGRESS_SCENE,
    &EDIT_BUTTON,
    &PASSWORD_EDIT_BUTTON,
    &ARCADE_C_CATEGORY_SCENE,
    &ARCADE_C_FRIEND_IMAGE_CTRL,
    &ARCADE_C_TITLE_SCENE_C_GAME_SCENE,
    &ARCADE_C_TITLE_SCENE_C_ACHIEVEMENT_SCENE,
    &ARCADE_C_TITLE_SCENE_C_LEADERBOARD_SCENE,
    &ARCADE_C_TITLE_SCENE_C_PRESENCE_SCENE,
    &ARCADE_C_TITLE_SCENE,
    &ARCADE_C_TITLE_ACHIEVEMENTS_SCENE,
    &ARCADE_C_COMPARE_ACHIEVEMENTS_SCENE,
    &ARCADE_C_TITLE_LEADERBOARD_SCENE,
    &ARCADE_C_TELL_A_FRIEND_SCENE,
    &ARCADE_C_TITLE_LAUNCH_SCENE,
    &ARCADE_C_TITLE_SELECTOR_SCENE,
    &ARCADE_C_CATEGORY_SELECTOR_SCENE,
    &ARCADE_CEZ_PASS_SCENE,
    &ARCADE_C_FRIENDS_PLAYING_NOW_SCENE,
    &ARCADE_C_HOMEPAGE_SCENE,
    &ARCADE_C_DOWNLOAD_META_SCENE,
    &ARCADE_C_MY_ARCADE_META_SCENE,
    &ARCADE_C_FRIENDS_META_SCENE,
    &ARCADE_C_TWIST_SELECTOR_SCENE,
    &ARCADE_C_RECENT_GAME_META_SCENE,
    &ARCADE_C_CATEGORY_LIST_CTRL,
    &ARCADE_C_TITLE_LIST_CTRL,
    &ARCADE_C_LEADERBOARD_LIST_CTRL,
    &ARCADE_C_TELL_A_FRIEND_LIST_CTRL,
    &ARCADE_C_FRIENDS_LIST_CTRL,
    &ARCADE_C_MULTI_TEXT_NAV_BUTTON,
    &ARCADE_C_ACHIEVEMENT_LIST_CTRL,
    &C_GAME_SHOWCASE_LIST_LIST,
    &C_GAME_SHOWCASE_SCENE,
    &C_ACHIEVEMENTS_GRID_TITLE_LIST,
    &C_ACHIEVEMENTS_GRID_LIST,
    &C_ACHIEVEMENTS_GRID_SCENE,
    &C_GAME_DETAILS_SCENE,
    &C2002_NETWORK_CONFIG_ACTION,
    &C2012_TEST_INFORMATION,
    &C2013_WINDOWS_CONNECT_NOW,
    &C2016_EDIT_IP_SETTINGS,
    &C2018_IP_SETTINGS,
    &C2033_DNS_CONFIG,
    &C2034_DNS_SETTINGS,
    &C2036_PP_PO_E_SETTINGS,
    &C2038_CONSOLE_INFORMATION,
    &C_NETWORK_LIST_ITEM,
    &C2020_AVAILABLE_WIRELESS_NETWORKS,
    &C2022_WIRELESS_SECURITY_SETTINGS,
    &C2024_ENTER_SETTINGS,
    &C2026_CHANNEL_LIST,
    &C2028_WIRELESS_MODES,
    &C2015_WIRELESS_INFORMATION,
    &C_CONN_STATUS,
    &C4_LINE_BTN,
    &C2004_MASTER_TAB,
    &C2004_NETWORK_DETAILS,
    &C2005_ADDITIONAL_SETTINGS,
    &C2010_TESTING_NETWORK,
    &C_TEST_PROFILE_ITEM,
    &C2011_TESTING_LAN,
    &STATUS_SCENE,
    &XUI_BOTD_OFFLINE_SCENE,
    &XUI_BOTD_OFFLINE_CONTAINER,
    &DEVICE_SELECTOR_LIST,
    &DEVICE_SELECTOR_SCENE,
    &DEVICE_DETAILS_SCENE,
    &CATEGORIES_LIST,
    &CATEGORIES_SCENE,
    &ITEMS_LIST,
    &ITEMS_SCENE,
    &ITEM_DETAILS_SCENE,
    &OPERATION_PROGRESS_SCENE,
    &FORMAT_HD_SCENE,
    &DELETE_MUSIC_SCENE,
    &HDDVD_DEVICE_SCENE,
    &HDDVD_CONTENTS_LIST,
    &HDDVD_CONTENTS_SCENE,
    &HDDVD_CONTENT_DETAILS_SCENE,
    &DOWNLOAD_LIVE_SIGNUP_SCENE,
    &CONGRATULATIONS_SCENE,
    &CI420_VIDEO_STREAM_XUI_CONTROL,
    &VIDEO_FEED_ZOOM_CONTROL,
    &VIDEO_CHAT_MAIN_SCENE,
    &DASH_IPTV_SETTINGS_SCENE,
    &DVR_STORAGE_SCENE,
    &UNINSTALL_IPTV_SCENE,
    &CONSOLE_SERIAL_NUM_EDIT,
    &MEDIA_TWIST,
    &ASYNC_TASK_MANAGER0,
    &BASE_SCENE,
    &EXTENDED_SCENE,
    &REPUTATION_SCENE,
    &XUI_GAMER_CARD,
    &GAMER_CARD_SCENE,
    &ME_GAMERCARD_WORKER,
    &YOU_GAMERCARD_WORKER,
    &ERROR_SCENE,
    &PC_SOFTWARE_SCENE,
    &WELCOME_SCENE,
    &NO_NETWORK_SCENE,
    &SLOW_NETWORK_SCENE,
    &RECONFIGURE_WIRELESS_SCENE,
    &WIRELESS80211B_SCENE,
    &LIMITED_NETWORK_SCENE,
    &NETWORK_SETTINGS_SCENE,
    &MEDIA_CENTER_SCENE,
    &CONTACT_ERROR_SCENE,
    &OLD_VERSION_SCENE,
    &WAITING_SCENE,
    &IPTV_CONNECTING_SCENE,
    &DASH_LIVE_SCENE,
    &DASH_LIVE_SIGNED_IN,
    &DASH_LIVE_SIGNED_OUT,
    &DASH_MP_SCENE,
    &C_GAMES_BLADE_SCENE,
    &C_GAMES_BLADE_SIGNED_IN,
    &C_GAMES_BLADE_SIGNED_OUT,
    &C_GAMES_BLADE_NAV_BUTTON,
    &DASH_MEDIA_SCENE,
    &MEDIA_BLADE_SIGNED_IN_SCENE,
    &MEDIA_BLADE_SIGNED_OUT_SCENE,
    &MEDIA_BLADE_META_IPTV,
    &OOBE_SCENE,
    &OOBE_COUNTRY,
    &OOBE_LANGUAGE,
    &C_GAMER_TILE,
    &C_RESOLUTION_CHOICE,
    &C_PAL_CHOICE,
    &C_STORAGE_DEVICE,
    &OOBE_PROFILE_CREATION,
    &C_SETUP_COMPLETE,
    &C_WELCOME_VIDEO_SCENE,
    &C_PROFILE_CHOICE,
    &GAMER_TILE_LIST,
    &DVD_INFO_PANE,
    &XMF_PREBUFFERING_SCENE,
    &DVD_PAD_DISPLAY,
    &DVD_PAD_PLAYBACK,
    &DVD_RC_DISPLAY,
    &DVD_HELP_PANE,
    &SCRIPT_DATA,
    &SCRIPT_SCENE,
    &SCRIPT_BUTTON,
    &SCRIPT_LIST,
    &SCRIPT_IMAGE,
    &DASH_DATA_LIST,
    &DASH_DATA,
    &VIDEO_CONTENT_LIST,
    &VIDEO_CONTENT_SCENE,
    &VIDEO_DATA,
    &VIDEO_CATEGORIES_LIST,
    &VIDEOS_DETAILS_TWIST,
    &VIDEOS_TWIST_FILTER,
    &ARCADE_TWIST_FILTER,
];

/// Registers every dashboard class. Their bases outside this module are all
/// `XuiInit` built-ins.
pub fn register(registry: &mut Registry) -> Result<(), RegisterError> {
    registry.register_all(&CLASSES)
}

/// Registered at 0x9210c3f0.
pub static LIVE_VISION_CONTROL: ClassDef =
    ClassDef::new("LiveVisionControl", Some("XuiControl"), &[]);

/// Registered at 0x921108a8.
pub static LIVE_VISION_DASH_EFFECT: ClassDef =
    ClassDef::new("LiveVisionDashEffect", Some("XuiControl"), &[]);

/// Registered at 0x92111b68.
pub static DASH_MAIN_SCENE: ClassDef = ClassDef::new("DashMainScene", Some("XuiTabScene"), &[]);

/// Registered at 0x92111e10.
pub static DVD_TRAY_SCENE: ClassDef = ClassDef::new("DVDTrayScene", Some("XuiScene"), &[]);

/// Registered at 0x92111ea8.
pub static TRAY_SCENE_BUTTON: ClassDef = ClassDef::new("TraySceneButton", Some("XuiButton"), &[]);

/// Registered at 0x92111f40.
pub static TRAY_SCENE_LOADER: ClassDef = ClassDef::new("TraySceneLoader", Some("XuiScene"), &[]);

/// Registered at 0x92111fd8.
pub static C_TITLE_META_DATA_SCENE: ClassDef =
    ClassDef::new("CTitleMetaDataScene", Some("XuiScene"), &[]);

/// Registered at 0x921120e0.
pub static C_XUI_EDIT_AUTO_SCROLL: ClassDef =
    ClassDef::new("CXuiEditAutoScroll", Some("XuiEdit"), &[]);

/// Registered at 0x92112178.
pub static MEDIA_PACKAGE: ClassDef = ClassDef::new("MediaPackage", Some("XuiScene"), &[]);

/// Registered at 0x92112220.
pub static C_LIVE_MESSAGE_BOX: ClassDef = ClassDef::new("CLiveMessageBox", Some("XuiElement"), &[]);

/// Registered at 0x921122b8.
pub static META_PANEL_SCENE: ClassDef = ClassDef::new("MetaPanelScene", Some("XuiScene"), &[]);

static DASH_SCENE_PROPS: [PropDef; 3] = [
    PropDef::new(
        "PanelSettings",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "PanelStrings",
        PropType::String,
        PropFlags::from_bits(0x4),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "PanelScenePaths",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
];

/// Registered at 0x92112350.
pub static DASH_SCENE: ClassDef = ClassDef::new("DashScene", Some("XuiScene"), &DASH_SCENE_PROPS);

/// Registered at 0x921123e8.
pub static C_WAIT_FOR_LOGON_SCENE: ClassDef =
    ClassDef::new("CWaitForLogonScene", Some("DashScene"), &[]);

/// Registered at 0x92112480.
pub static NETWORK_DEVICE_SELECT_SCENE: ClassDef =
    ClassDef::new("NetworkDeviceSelectScene", Some("DashScene"), &[]);

/// Registered at 0x92112518.
pub static NETWORK_DEVICE_META_PANEL_SCENE: ClassDef =
    ClassDef::new("NetworkDeviceMetaPanelScene", Some("XuiScene"), &[]);

/// Registered at 0x921125b0.
pub static NETWORK_DEVICE_MANAGEMENT_SCENE: ClassDef =
    ClassDef::new("NetworkDeviceManagementScene", Some("XuiScene"), &[]);

/// Registered at 0x92112648.
pub static DASH_SYSTEM_SCENE: ClassDef = ClassDef::new("DashSystemScene", Some("DashScene"), &[]);

/// Registered at 0x921126e0.
pub static WALLPAPER: ClassDef = ClassDef::new("Wallpaper", Some("XuiControl"), &[]);

/// Registered at 0x92112778.
pub static DASH_SIGN_OUT: ClassDef = ClassDef::new("dashSignOut", Some("XuiNavButton"), &[]);

/// Registered at 0x92112810.
pub static C_DASH_LIST_CHOOSER: ClassDef =
    ClassDef::new("CDashListChooser", Some("XuiCommonList"), &[]);

/// Registered at 0x921abe88.
pub static MEDIA_SOURCE_NO_COMPUTERS_METAPANEL_SCENE: ClassDef = ClassDef::new(
    "MediaSourceNoComputersMetapanelScene",
    Some("XuiScene"),
    &[],
);

/// Registered at 0x921ac3e0.
pub static MEDIA_SOURCE_LIST: ClassDef = ClassDef::new("MediaSourceList", Some("XuiList"), &[]);

static DASH_VIDEO_PROPS: [PropDef; 2] = [
    PropDef::new(
        "File",
        PropType::String,
        PropFlags::from_bits(0x10),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "SizeMode",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x921b01c0.
pub static DASH_VIDEO: ClassDef = ClassDef::new("DashVideo", Some("XuiElement"), &DASH_VIDEO_PROPS);

/// Registered at 0x921b6000.
pub static DASH_DATA_LIST: ClassDef = ClassDef::new("DashDataList", Some("ScriptList"), &[]);

/// Registered at 0x921b6430.
pub static DASH_DATA: ClassDef = ClassDef::new("DashData", Some("ScriptData"), &[]);

/// Registered at 0x921ba180.
pub static C_BLADE_MOTD: ClassDef = ClassDef::new("CBladeMOTD", Some("XuiScene"), &[]);

/// Registered at 0x921ba218.
pub static DASH_BLADE_TAB: ClassDef = ClassDef::new("DashBladeTab", Some("XuiScene"), &[]);

/// Registered at 0x921c1b58.
pub static DASH_SETTINGS_ROOT: ClassDef = ClassDef::new("dashSettingsRoot", Some("DashScene"), &[]);

/// Registered at 0x921c1bf0.
pub static DASH_C_VALUE_SPIN: ClassDef = ClassDef::new("dashCValueSpin", Some("XuiList"), &[]);

/// Registered at 0x921c1d78.
pub static DASH_C_DATE: ClassDef = ClassDef::new("dashCDate", Some("XuiScene"), &[]);

/// Registered at 0x921c1e80.
pub static DASH_C_TIMEZONE: ClassDef = ClassDef::new("dashCTimezone", Some("XuiList"), &[]);

/// Registered at 0x921c1f18.
pub static DASH_C_LANGUAGE: ClassDef = ClassDef::new("dashCLanguage", Some("XuiList"), &[]);

/// Registered at 0x921c1fb0.
pub static DASH_C_COUNTRY: ClassDef = ClassDef::new("dashCCountry", Some("XuiList"), &[]);

/// Registered at 0x921c2048.
pub static DASH_C_RATING_VIEW: ClassDef = ClassDef::new("dashCRatingView", Some("XuiList"), &[]);

/// Registered at 0x921c20e0.
pub static DASH_C_SCREEN_SIZE: ClassDef = ClassDef::new("dashCScreenSize", Some("XuiList"), &[]);

/// Registered at 0x921c2360.
pub static DASH_C_HINTS: ClassDef = ClassDef::new("dashCHints", Some("XuiList"), &[]);

/// Registered at 0x921c27b8.
pub static DASH_REMOTE_C_LIST: ClassDef = ClassDef::new("dashRemoteCList", Some("XuiList"), &[]);

/// Registered at 0x921c2bf0.
pub static DASH_SETTINGS_LIST: ClassDef =
    ClassDef::new("DashSettingsList", Some("XuiCommonList"), &[]);

/// Registered at 0x921c2c88.
pub static DASH_P_CONTROL_SELECT: ClassDef =
    ClassDef::new("dashPControlSelect", Some("DashScene"), &[]);

/// Registered at 0x921c2e00.
pub static DASH_P_CONTROL_SETTINGS_VIDEO: ClassDef =
    ClassDef::new("dashPControlSettings_Video", Some("DashScene"), &[]);

/// Registered at 0x921c2e98.
pub static DASH_P_CONTROL_SETTINGS_VIDEO_MOVIE: ClassDef =
    ClassDef::new("dashPControlSettings_VideoMovie", Some("DashScene"), &[]);

/// Registered at 0x921c2f30.
pub static DASH_P_CONTROL_SETTINGS_VIDEO_TV: ClassDef =
    ClassDef::new("dashPControlSettings_VideoTV", Some("DashScene"), &[]);

/// Registered at 0x921c3038.
pub static DASH_P_CONTROL_SETTINGS_VIDEO_UNRATED: ClassDef =
    ClassDef::new("dashPControlSettings_VideoUnrated", Some("DashScene"), &[]);

/// Registered at 0x921c30d0.
pub static DASH_P_CONTROL_SETTINGS_LIVE_A: ClassDef =
    ClassDef::new("dashPControlSettings_LiveA", Some("XuiScene"), &[]);

/// Registered at 0x921c3168.
pub static DASH_P_CONTROL_SETTINGS_CONTENT: ClassDef =
    ClassDef::new("dashPControlSettings_Content", Some("XuiScene"), &[]);

/// Registered at 0x921c3200.
pub static DASH_P_CONTROL_SETTINGS_LIVE_C: ClassDef =
    ClassDef::new("dashPControlSettings_LiveC", Some("XuiScene"), &[]);

/// Registered at 0x921c3298.
pub static DASH_P_CONTROL_SETTINGS_PASSCODE: ClassDef =
    ClassDef::new("dashPControlSettings_Passcode", Some("XuiScene"), &[]);

/// Registered at 0x921c3330.
pub static DASH_P_CONTROL_SETTINGS_PASSCODE_HINT: ClassDef =
    ClassDef::new("dashPControlSettings_PasscodeHint", Some("XuiScene"), &[]);

/// Registered at 0x921c33c8.
pub static DASH_PASSCODE_EDIT: ClassDef = ClassDef::new("dashPasscodeEdit", Some("XuiButton"), &[]);

/// Registered at 0x921c3460.
pub static DASH_PASSCODE_N_EDIT: ClassDef =
    ClassDef::new("dashPasscodeNEdit", Some("XuiNavButton"), &[]);

/// Registered at 0x921c34f8.
pub static C_FAMILY_TIMER_DURATION_LIST: ClassDef =
    ClassDef::new("CFamilyTimerDurationList", Some("XuiList"), &[]);

/// Registered at 0x921c3590.
pub static C_FAMILY_TIMER_SETTINGS_SCENE: ClassDef =
    ClassDef::new("CFamilyTimerSettingsScene", Some("DashScene"), &[]);

/// Registered at 0x921c3628.
pub static DASH_C_TEXT_SPIN: ClassDef = ClassDef::new("dashCTextSpin", Some("XuiList"), &[]);

/// Registered at 0x921c36c0.
pub static DASH_C_TIME: ClassDef = ClassDef::new("dashCTime", Some("XuiScene"), &[]);

/// Registered at 0x921c3758.
pub static DASH_CC_INPUT_SCENE: ClassDef = ClassDef::new("dashCCInputScene", Some("XuiScene"), &[]);

/// Registered at 0x921c37f0.
pub static DASH_VIDEO_SETTINGS: ClassDef =
    ClassDef::new("dashVideoSettings", Some("DashScene"), &[]);

/// Registered at 0x921c3888.
pub static DASH_VIDEO_SETTINGS_HD: ClassDef =
    ClassDef::new("dashVideoSettings_HD", Some("XuiScene"), &[]);

/// Registered at 0x921c3920.
pub static DASH_VIDEO_SETTINGS_FORMAT: ClassDef =
    ClassDef::new("dashVideoSettings_Format", Some("DashScene"), &[]);

/// Registered at 0x921c39b8.
pub static DASH_VIDEO_SETTINGS_60: ClassDef =
    ClassDef::new("dashVideoSettings_60", Some("XuiScene"), &[]);

/// Registered at 0x921c3ac0.
pub static DASH_AUDIO_SETTINGS_D: ClassDef =
    ClassDef::new("dashAudioSettings_D", Some("XuiScene"), &[]);

/// Registered at 0x921c3b58.
pub static DASH_AUDIO_SETTINGS_FX: ClassDef =
    ClassDef::new("dashAudioSettings_FX", Some("XuiScene"), &[]);

/// Registered at 0x921c3c60.
pub static DASH_CLOCK_SETTINGS_MENU: ClassDef =
    ClassDef::new("dashClockSettings_Menu", Some("DashScene"), &[]);

/// Registered at 0x921c3cf8.
pub static DASH_CLOCK_SETTINGS: ClassDef =
    ClassDef::new("dashClockSettings", Some("XuiScene"), &[]);

/// Registered at 0x921c3d90.
pub static DASH_CLOCK_SETTINGS_T_ZONE: ClassDef =
    ClassDef::new("dashClockSettings_TZone", Some("XuiScene"), &[]);

/// Registered at 0x921c3ea8.
pub static DASH_CLOCK_SETTINGS_1224: ClassDef =
    ClassDef::new("dashClockSettings_1224", Some("XuiScene"), &[]);

/// Registered at 0x921c3f40.
pub static DASH_COUNTRY_SETTINGS: ClassDef =
    ClassDef::new("dashCountrySettings", Some("XuiScene"), &[]);

/// Registered at 0x921c3fd8.
pub static DASH_AUTO_OFF: ClassDef = ClassDef::new("dashAutoOff", Some("XuiScene"), &[]);

/// Registered at 0x921c4070.
pub static DASH_REMOTE_C: ClassDef = ClassDef::new("dashRemoteC", Some("XuiScene"), &[]);

/// Registered at 0x921c4108.
pub static DASH_SCREENSAVER: ClassDef = ClassDef::new("dashScreensaver", Some("XuiScene"), &[]);

/// Registered at 0x921c41a0.
pub static DASH_OUTPUT_LEVELS: ClassDef = ClassDef::new("dashOutputLevels", Some("XuiScene"), &[]);

/// Registered at 0x921c4238.
pub static DASH_SYSTEM_RESET: ClassDef = ClassDef::new("dashSystemReset", Some("XuiScene"), &[]);

/// Registered at 0x921c42d0.
pub static DASH_START_UP: ClassDef = ClassDef::new("dashStartUp", Some("XuiScene"), &[]);

/// Registered at 0x921c4368.
pub static DASH_LIVE_VISION_SETTINGS_SCENE: ClassDef =
    ClassDef::new("dashLiveVisionSettingsScene", Some("DashScene"), &[]);

/// Registered at 0x921c4400.
pub static DASH_BACKGROUND_DOWNLOADS: ClassDef =
    ClassDef::new("dashBackgroundDownloads", Some("XuiScene"), &[]);

/// Registered at 0x921c4498.
pub static DASH_SHUTDOWN_SETTINGS: ClassDef =
    ClassDef::new("dashShutdownSettings", Some("DashScene"), &[]);

/// Registered at 0x921c4530.
pub static DASH_P_CONTROL_SETTINGS_MENU: ClassDef =
    ClassDef::new("dashPControlSettingsMenu", Some("DashScene"), &[]);

/// Registered at 0x921c45c8.
pub static DASH_P_CONTROL_SETTINGS_GAME: ClassDef =
    ClassDef::new("dashPControlSettings_Game", Some("DashScene"), &[]);

/// Registered at 0x921c4660.
pub static DASH_P_CONTROL_SETTINGS_VIDEO_EXPLICIT: ClassDef =
    ClassDef::new("dashPControlSettings_VideoExplicit", Some("DashScene"), &[]);

/// Registered at 0x921c46f8.
pub static DASH_AUDIO_SETTINGS: ClassDef =
    ClassDef::new("dashAudioSettings", Some("DashScene"), &[]);

/// Registered at 0x921c4790.
pub static DASH_LANGUAGE_SETTINGS: ClassDef =
    ClassDef::new("dashLanguageSettings", Some("XuiScene"), &[]);

/// Registered at 0x921c4828.
pub static DASH_CLOCK_SETTINGS_D_SAVING: ClassDef =
    ClassDef::new("dashClockSettings_DSaving", Some("XuiScene"), &[]);

/// Registered at 0x921ce698.
pub static MEDIA_SOURCE_SELECTION_SCENE: ClassDef =
    ClassDef::new("MediaSourceSelectionScene", Some("DashScene"), &[]);

/// Registered at 0x921ce7b0.
pub static GRID_LIST: ClassDef = ClassDef::new("GridList", Some("XuiList"), &[]);

/// Registered at 0x921ce8c8.
pub static TRANSPORT_CONTROLS_SCENE: ClassDef =
    ClassDef::new("TransportControlsScene", Some("XuiScene"), &[]);

/// Registered at 0x921ce960.
pub static IMAGE_RENDER_CONTROL: ClassDef =
    ClassDef::new("ImageRenderControl", Some("XuiControl"), &[]);

/// Registered at 0x921cebf8.
pub static DEVICE_MAIN_SCENE: ClassDef = ClassDef::new("DeviceMainScene", Some("XuiScene"), &[]);

/// Registered at 0x921cec90.
pub static FULL_SCREEN_PHOTO_SCENE: ClassDef =
    ClassDef::new("FullScreenPhotoScene", Some("XuiScene"), &[]);

/// Registered at 0x921d8680.
pub static SONG_LIST: ClassDef = ClassDef::new("SongList", Some("XuiList"), &[]);

/// Registered at 0x921d8718.
pub static MEDIA_CONTAINER_LIST: ClassDef =
    ClassDef::new("MediaContainerList", Some("XuiList"), &[]);

/// Registered at 0x921d87b0.
pub static MUSIC_PLAY_PAUSE_BUTTON: ClassDef =
    ClassDef::new("MusicPlayPauseButton", Some("XuiButton"), &[]);

/// Registered at 0x921d88c8.
pub static GOTO_MUSIC_PLAYER_SCENE: ClassDef =
    ClassDef::new("GotoMusicPlayerScene", Some("XuiScene"), &[]);

/// Registered at 0x921d8960.
pub static GOTO_MUSIC_PLAYER_DASH_SCENE: ClassDef =
    ClassDef::new("GotoMusicPlayerDashScene", Some("DashScene"), &[]);

/// Registered at 0x921d8a78.
pub static SONG_LIST_SCENE: ClassDef = ClassDef::new("SongListScene", Some("XuiScene"), &[]);

/// Registered at 0x921d8b10.
pub static MEDIA_CONTAINER_LIST_SCENE: ClassDef =
    ClassDef::new("MediaContainerListScene", Some("XuiScene"), &[]);

/// Registered at 0x921d8ba8.
pub static MEDIA_CONTAINER_OPTIONS_SCENE: ClassDef = ClassDef::new(
    "MediaContainerOptionsScene",
    Some("GotoMusicPlayerScene"),
    &[],
);

/// Registered at 0x921d8c40.
pub static ARTIST_OPTIONS_SCENE: ClassDef =
    ClassDef::new("ArtistOptionsScene", Some("GotoMusicPlayerScene"), &[]);

/// Registered at 0x921d8d58.
pub static SONG_OPTIONS_SCENE: ClassDef =
    ClassDef::new("SongOptionsScene", Some("GotoMusicPlayerScene"), &[]);

/// Registered at 0x921d8df0.
pub static MUSIC_LIBRARY_SCENE: ClassDef =
    ClassDef::new("MusicLibraryScene", Some("GotoMusicPlayerDashScene"), &[]);

/// Registered at 0x921d8e88.
pub static EDIT_NOW_PLAYING_QUEUE_SCENE: ClassDef =
    ClassDef::new("EditNowPlayingQueueScene", Some("XuiScene"), &[]);

/// Registered at 0x921d8f20.
pub static EDIT_NOW_PLAYING_QUEUE_LIST: ClassDef =
    ClassDef::new("EditNowPlayingQueueList", Some("XuiList"), &[]);

/// Registered at 0x921d9038.
pub static RIP_CD_PROGRESS_LIST: ClassDef =
    ClassDef::new("RipCDProgressList", Some("XuiList"), &[]);

/// Registered at 0x921d91d0.
pub static EDIT_BOX_BUTTON: ClassDef = ClassDef::new("EditBoxButton", Some("XuiNavButton"), &[]);

/// Registered at 0x921d9268.
pub static NOW_PLAYING_SCENE: ClassDef = ClassDef::new("NowPlayingScene", Some("XuiScene"), &[]);

/// Registered at 0x921d9500.
pub static MUSIC_EQUALIZER: ClassDef = ClassDef::new("MusicEqualizer", Some("XuiLabel"), &[]);

/// Registered at 0x921d9598.
pub static NOW_PLAYING_LIST: ClassDef = ClassDef::new("NowPlayingList", Some("XuiList"), &[]);

/// Registered at 0x921d9630.
pub static FILE_VIEW_SCENE: ClassDef =
    ClassDef::new("FileViewScene", Some("GotoMusicPlayerScene"), &[]);

/// Registered at 0x921d96c8.
pub static FILE_VIEW_LIST: ClassDef = ClassDef::new("FileViewList", Some("XuiList"), &[]);

/// Registered at 0x921d9a50.
pub static MUSIC_PLAYBACK_MODE_BUTTON: ClassDef =
    ClassDef::new("MusicPlaybackModeButton", Some("XuiButton"), &[]);

/// Registered at 0x921d9ae8.
pub static DEVICE_NOT_BROWSEABLE_SCENE: ClassDef = ClassDef::new(
    "DeviceNotBrowseableScene",
    Some("GotoMusicPlayerScene"),
    &[],
);

/// Registered at 0x921d9b80.
pub static SONG_METADATA_SCENE: ClassDef =
    ClassDef::new("SongMetadataScene", Some("XuiScene"), &[]);

/// Registered at 0x921d9c18.
pub static RIP_CD_PROGRESS_SCENE: ClassDef =
    ClassDef::new("RipCDProgressScene", Some("XuiScene"), &[]);

/// Registered at 0x921d9cb0.
pub static RIP_CD_SELECT_SCENE: ClassDef = ClassDef::new("RipCDSelectScene", Some("XuiScene"), &[]);

/// Registered at 0x921d9d48.
pub static EDIT_TAG_INFO_SCENE: ClassDef = ClassDef::new("EditTagInfoScene", Some("XuiScene"), &[]);

/// Registered at 0x921d9de0.
pub static NOW_PLAYING_RIP_OR_EDIT_BUTTON: ClassDef =
    ClassDef::new("NowPlayingRipOrEditButton", Some("XuiButton"), &[]);

/// Registered at 0x921d9e78.
pub static VISUALIZER_BUTTON: ClassDef = ClassDef::new("VisualizerButton", Some("XuiButton"), &[]);

/// Registered at 0x921d9f10.
pub static VISUALIZER_SCENE: ClassDef = ClassDef::new("VisualizerScene", Some("XuiScene"), &[]);

/// Registered at 0x921d9fa8.
pub static MUSIC_TRANSPORT_CONTROLS_SCENE: ClassDef =
    ClassDef::new("MusicTransportControlsScene", Some("XuiScene"), &[]);

/// Registered at 0x921ed740.
pub static VIDEO_CONTENT_LIST: ClassDef =
    ClassDef::new("VideoContentList", Some("DashDataList"), &[]);

/// Registered at 0x921edb48.
pub static VIDEO_SCENE: ClassDef = ClassDef::new("VideoScene", Some("XuiScene"), &[]);

/// Registered at 0x921edc50.
pub static VIDEO_CONTENT_SCENE: ClassDef =
    ClassDef::new("VideoContentScene", Some("ScriptScene"), &[]);

/// Registered at 0x921edce8.
pub static VIDEO_DATA: ClassDef = ClassDef::new("VideoData", Some("DashData"), &[]);

/// Registered at 0x921edd80.
pub static VIDEO_CATEGORIES_LIST: ClassDef =
    ClassDef::new("VideoCategoriesList", Some("DashDataList"), &[]);

/// Registered at 0x921ede60.
pub static VIDEOS_DETAILS_TWIST: ClassDef =
    ClassDef::new("VideosDetailsTwist", Some("MediaTwist"), &[]);

/// Registered at 0x921edf78.
pub static VIDEOS_TWIST_FILTER: ClassDef =
    ClassDef::new("VideosTwistFilter", Some("MediaTwist"), &[]);

/// Registered at 0x921fad50.
pub static C_POSTAL_DATA_LIST: ClassDef = ClassDef::new("CPostalDataList", Some("XuiList"), &[]);

/// Registered at 0x921fade8.
pub static C_ADDRESS_BUTTON: ClassDef = ClassDef::new("CAddressButton", Some("XuiButton"), &[]);

/// Registered at 0x921fb080.
pub static C_STATE_LIST: ClassDef = ClassDef::new("CStateList", Some("XuiList"), &[]);

/// Registered at 0x921fb1e0.
pub static C_CONTACT_ADDRESS_SCENE: ClassDef =
    ClassDef::new("CContactAddressScene", Some("DashScene"), &[]);

/// Registered at 0x921fb578.
pub static C_CHANGE_LIVE_ID_CURRENT_SIGN_IN: ClassDef =
    ClassDef::new("CChangeLiveIDCurrentSignIn", Some("DashScene"), &[]);

/// Registered at 0x921fb610.
pub static C_CHANGE_LIVE_ID_NEW_SIGN_IN: ClassDef =
    ClassDef::new("CChangeLiveIDNewSignIn", Some("DashScene"), &[]);

/// Registered at 0x921fb6a8.
pub static C_CHANGE_LIVE_ID_PARENT_SIGN_IN: ClassDef =
    ClassDef::new("CChangeLiveIDParentSignIn", Some("DashScene"), &[]);

/// Registered at 0x921fb958.
pub static C_PRIV_TWO_OPTIONS_SCENE: ClassDef =
    ClassDef::new("CPrivTwoOptionsScene", Some("DashScene"), &[]);

/// Registered at 0x921fb9f0.
pub static C_PRIV_THREE_OPTIONS_SCENE: ClassDef =
    ClassDef::new("CPrivThreeOptionsScene", Some("DashScene"), &[]);

/// Registered at 0x921fbfb0.
pub static C_LIVE_KEYBOARD: ClassDef = ClassDef::new("CLiveKeyboard", Some("XuiElement"), &[]);

/// Registered at 0x921fc048.
pub static CAM_HOME_SCENE: ClassDef = ClassDef::new("CAMHomeScene", Some("DashScene"), &[]);

/// Registered at 0x921fc0e0.
pub static CAM_MY_MEMBERSHIPS_SCENE: ClassDef =
    ClassDef::new("CAMMyMembershipsScene", Some("DashScene"), &[]);

/// Registered at 0x921fc178.
pub static C_MY_MEMBERSHIPS_LIST: ClassDef =
    ClassDef::new("CMyMembershipsList", Some("XuiList"), &[]);

/// Registered at 0x921fc210.
pub static C_MY_MEMBERSHIPS_SCENE: ClassDef =
    ClassDef::new("CMyMembershipsScene", Some("XuiScene"), &[]);

/// Registered at 0x921fc390.
pub static C_PASSPORT_SIGN_IN_BUTTON: ClassDef =
    ClassDef::new("CPassportSignInButton", Some("XuiButton"), &[]);

/// Registered at 0x921fc428.
pub static C_PASSPORT_SIGN_IN_SCENE: ClassDef =
    ClassDef::new("CPassportSignInScene", Some("DashScene"), &[]);

/// Registered at 0x921fc4c0.
pub static C_PASSPORT_PASSWORD_BUTTON: ClassDef =
    ClassDef::new("CPassportPasswordButton", Some("XuiButton"), &[]);

/// Registered at 0x921fc558.
pub static C_PASSPORT_PASSWORD_SCENE: ClassDef =
    ClassDef::new("CPassportPasswordScene", Some("DashScene"), &[]);

/// Registered at 0x921fc5f0.
pub static CWLID_MENU_SCENE: ClassDef = ClassDef::new("CWLIDMenuScene", Some("DashScene"), &[]);

/// Registered at 0x921fc688.
pub static C_CHANGE_LIVE_ID_MENU_SCENE: ClassDef =
    ClassDef::new("CChangeLiveIDMenuScene", Some("DashScene"), &[]);

/// Registered at 0x921fc720.
pub static C_CHANGE_LIVE_ID_FINISH_SCENE: ClassDef =
    ClassDef::new("CChangeLiveIDFinishScene", Some("DashScene"), &[]);

/// Registered at 0x921fc7b8.
pub static C_CHANGE_LIVE_ID_CONGRATS_SCENE: ClassDef =
    ClassDef::new("CChangeLiveIDCongratsScene", Some("DashScene"), &[]);

/// Registered at 0x921fc850.
pub static PRIVACY_SETTINGS_LIST: ClassDef =
    ClassDef::new("PrivacySettingsList", Some("XuiCommonList"), &[]);

/// Registered at 0x921fc8e8.
pub static C_PRIVACY_MAIN_SCENE: ClassDef =
    ClassDef::new("CPrivacyMainScene", Some("DashScene"), &[]);

/// Registered at 0x921fc980.
pub static C_MARKETING_TWO_OPTIONS_SCENE: ClassDef =
    ClassDef::new("CMarketingTwoOptionsScene", Some("DashScene"), &[]);

/// Registered at 0x921fca18.
pub static C_POLICY_MAIN_SCENE: ClassDef =
    ClassDef::new("CPolicyMainScene", Some("DashScene"), &[]);

/// Registered at 0x921fcab0.
pub static C_POLICY_DISPLAY_SCENE: ClassDef =
    ClassDef::new("CPolicyDisplayScene", Some("DashScene"), &[]);

/// Registered at 0x921fcb48.
pub static CPC_MAIN_SCENE: ClassDef = ClassDef::new("CPCMainScene", Some("DashScene"), &[]);

/// Registered at 0x921fcbe0.
pub static CPC_PRIV_FRIENDS_SCENE: ClassDef =
    ClassDef::new("CPCPrivFriendsScene", Some("DashScene"), &[]);

/// Registered at 0x921fcc78.
pub static C_GAMEPLAY_SETTINGS_SCENE: ClassDef =
    ClassDef::new("CGameplaySettingsScene", Some("DashScene"), &[]);

/// Registered at 0x921fcd10.
pub static CPC_CONTENT_SETTINGS_SCENE: ClassDef =
    ClassDef::new("CPCContentSettingsScene", Some("DashScene"), &[]);

/// Registered at 0x921fcda8.
pub static C_MORE_INFO_SCENE: ClassDef = ClassDef::new("CMoreInfoScene", Some("DashScene"), &[]);

/// Registered at 0x921fce40.
pub static C_ABOUT_MS_POINTS_LIST_SCENE: ClassDef =
    ClassDef::new("CAboutMSPointsListScene", Some("DashScene"), &[]);

/// Registered at 0x921fcf58.
pub static C_POSTAL_DATA_LIST_SCENE: ClassDef =
    ClassDef::new("CPostalDataListScene", Some("DashScene"), &[]);

/// Registered at 0x921fcff0.
pub static C_STATE_LIST_SCENE: ClassDef = ClassDef::new("CStateListScene", Some("DashScene"), &[]);

/// Registered at 0x9220e318.
pub static C_MESSENGER_WELCOME_SCENE: ClassDef =
    ClassDef::new("CMessengerWelcomeScene", Some("DashScene"), &[]);

/// Registered at 0x9220e3b0.
pub static C_MESSENGER_SELECT_WLID_SCENE: ClassDef =
    ClassDef::new("CMessengerSelectWLIDScene", Some("DashScene"), &[]);

/// Registered at 0x9220e448.
pub static C_MESSENGER_SIGNUP_COMPLETE_SCENE: ClassDef =
    ClassDef::new("CMessengerSignupCompleteScene", Some("DashScene"), &[]);

/// Registered at 0x9220e4e0.
pub static C_FRIEND_REQUEST_RECIPIENTS_LIST: ClassDef =
    ClassDef::new("CFriendRequestRecipientsList", Some("XuiCommonList"), &[]);

/// Registered at 0x9220e578.
pub static C_FRIEND_REQUEST_INTRO_SCENE: ClassDef =
    ClassDef::new("CFriendRequestIntroScene", Some("DashScene"), &[]);

/// Registered at 0x9220e610.
pub static C_FRIEND_REQUEST_MAIN_SCENE: ClassDef =
    ClassDef::new("CFriendRequestMainScene", Some("DashScene"), &[]);

/// Registered at 0x9220e6a8.
pub static FRIEND_REQUEST_PROGRESS_SCENE: ClassDef =
    ClassDef::new("FriendRequestProgressScene", Some("DashScene"), &[]);

/// Registered at 0x9220e740.
pub static EDIT_BUTTON: ClassDef = ClassDef::new("EditButton", Some("XuiButton"), &[]);

/// Registered at 0x9220e7d8.
pub static PASSWORD_EDIT_BUTTON: ClassDef =
    ClassDef::new("PasswordEditButton", Some("XuiButton"), &[]);

/// Registered at 0x92213bf0.
pub static ARCADE_C_CATEGORY_SCENE: ClassDef =
    ClassDef::new("Arcade::CCategoryScene", Some("XuiScene"), &[]);

/// Registered at 0x92213c88.
pub static ARCADE_C_FRIEND_IMAGE_CTRL: ClassDef =
    ClassDef::new("Arcade::CFriendImageCtrl", Some("XuiControl"), &[]);

/// Registered at 0x92213d20.
pub static ARCADE_C_TITLE_SCENE_C_GAME_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleScene::CGameScene", Some("XuiScene"), &[]);

/// Registered at 0x92213db8.
pub static ARCADE_C_TITLE_SCENE_C_ACHIEVEMENT_SCENE: ClassDef = ClassDef::new(
    "Arcade::CTitleScene::CAchievementScene",
    Some("XuiScene"),
    &[],
);

/// Registered at 0x92213e50.
pub static ARCADE_C_TITLE_SCENE_C_LEADERBOARD_SCENE: ClassDef = ClassDef::new(
    "Arcade::CTitleScene::CLeaderboardScene",
    Some("XuiScene"),
    &[],
);

/// Registered at 0x92213ee8.
pub static ARCADE_C_TITLE_SCENE_C_PRESENCE_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleScene::CPresenceScene", Some("XuiScene"), &[]);

/// Registered at 0x92213f80.
pub static ARCADE_C_TITLE_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleScene", Some("XuiScene"), &[]);

/// Registered at 0x92214018.
pub static ARCADE_C_TITLE_ACHIEVEMENTS_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleAchievementsScene", Some("XuiScene"), &[]);

/// Registered at 0x922140b0.
pub static ARCADE_C_COMPARE_ACHIEVEMENTS_SCENE: ClassDef =
    ClassDef::new("Arcade::CCompareAchievementsScene", Some("XuiScene"), &[]);

/// Registered at 0x92214148.
pub static ARCADE_C_TITLE_LEADERBOARD_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleLeaderboardScene", Some("XuiScene"), &[]);

/// Registered at 0x922141e0.
pub static ARCADE_C_TELL_A_FRIEND_SCENE: ClassDef =
    ClassDef::new("Arcade::CTellAFriendScene", Some("XuiScene"), &[]);

/// Registered at 0x92214278.
pub static ARCADE_C_TITLE_LAUNCH_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleLaunchScene", Some("XuiScene"), &[]);

/// Registered at 0x92214310.
pub static ARCADE_C_TITLE_SELECTOR_SCENE: ClassDef =
    ClassDef::new("Arcade::CTitleSelectorScene", Some("DashScene"), &[]);

/// Registered at 0x922143a8.
pub static ARCADE_C_CATEGORY_SELECTOR_SCENE: ClassDef =
    ClassDef::new("Arcade::CCategorySelectorScene", Some("DashScene"), &[]);

/// Registered at 0x92214440.
pub static ARCADE_CEZ_PASS_SCENE: ClassDef =
    ClassDef::new("Arcade::CEZPassScene", Some("DashScene"), &[]);

/// Registered at 0x922144d8.
pub static ARCADE_C_FRIENDS_PLAYING_NOW_SCENE: ClassDef =
    ClassDef::new("Arcade::CFriendsPlayingNowScene", Some("XuiScene"), &[]);

/// Registered at 0x92214570.
pub static ARCADE_C_HOMEPAGE_SCENE: ClassDef =
    ClassDef::new("Arcade::CHomepageScene", Some("DashScene"), &[]);

/// Registered at 0x92214678.
pub static ARCADE_C_DOWNLOAD_META_SCENE: ClassDef =
    ClassDef::new("Arcade::CDownloadMetaScene", Some("XuiScene"), &[]);

/// Registered at 0x92214710.
pub static ARCADE_C_MY_ARCADE_META_SCENE: ClassDef =
    ClassDef::new("Arcade::CMyArcadeMetaScene", Some("XuiScene"), &[]);

/// Registered at 0x922147a8.
pub static ARCADE_C_FRIENDS_META_SCENE: ClassDef =
    ClassDef::new("Arcade::CFriendsMetaScene", Some("XuiScene"), &[]);

/// Registered at 0x922148b0.
pub static ARCADE_C_TWIST_SELECTOR_SCENE: ClassDef =
    ClassDef::new("Arcade::CTwistSelectorScene", Some("DashScene"), &[]);

/// Registered at 0x92214ad8.
pub static ARCADE_TWIST_FILTER: ClassDef =
    ClassDef::new("ArcadeTwistFilter", Some("MediaTwist"), &[]);

/// Registered at 0x92214e48.
pub static ARCADE_C_RECENT_GAME_META_SCENE: ClassDef =
    ClassDef::new("Arcade::CRecentGameMetaScene", Some("XuiScene"), &[]);

/// Registered at 0x92214f60.
pub static ARCADE_C_CATEGORY_LIST_CTRL: ClassDef =
    ClassDef::new("Arcade::CCategoryListCtrl", Some("XuiList"), &[]);

/// Registered at 0x92214ff8.
pub static ARCADE_C_TITLE_LIST_CTRL: ClassDef =
    ClassDef::new("Arcade::CTitleListCtrl", Some("XuiList"), &[]);

/// Registered at 0x92215090.
pub static ARCADE_C_LEADERBOARD_LIST_CTRL: ClassDef =
    ClassDef::new("Arcade::CLeaderboardListCtrl", Some("XuiList"), &[]);

/// Registered at 0x92215128.
pub static ARCADE_C_TELL_A_FRIEND_LIST_CTRL: ClassDef =
    ClassDef::new("Arcade::CTellAFriendListCtrl", Some("XuiList"), &[]);

/// Registered at 0x922151c0.
pub static ARCADE_C_FRIENDS_LIST_CTRL: ClassDef =
    ClassDef::new("Arcade::CFriendsListCtrl", Some("XuiList"), &[]);

/// Registered at 0x92215258.
pub static ARCADE_C_MULTI_TEXT_NAV_BUTTON: ClassDef =
    ClassDef::new("Arcade::CMultiTextNavButton", Some("XuiNavButton"), &[]);

/// Registered at 0x922152f0.
pub static ARCADE_C_ACHIEVEMENT_LIST_CTRL: ClassDef =
    ClassDef::new("Arcade::CAchievementListCtrl", Some("XuiList"), &[]);

/// Registered at 0x92238568.
pub static C_GAME_SHOWCASE_LIST_LIST: ClassDef =
    ClassDef::new("CGameShowcaseListList", Some("XuiList"), &[]);

/// Registered at 0x92238600.
pub static C_GAME_SHOWCASE_SCENE: ClassDef =
    ClassDef::new("CGameShowcaseScene", Some("XuiScene"), &[]);

/// Registered at 0x92238698.
pub static C_ACHIEVEMENTS_GRID_TITLE_LIST: ClassDef =
    ClassDef::new("CAchievementsGridTitleList", Some("XuiList"), &[]);

/// Registered at 0x92238730.
pub static C_ACHIEVEMENTS_GRID_LIST: ClassDef =
    ClassDef::new("CAchievementsGridList", Some("XuiList"), &[]);

/// Registered at 0x922387c8.
pub static C_ACHIEVEMENTS_GRID_SCENE: ClassDef =
    ClassDef::new("CAchievementsGridScene", Some("DashScene"), &[]);

/// Registered at 0x92238860.
pub static C_GAME_DETAILS_SCENE: ClassDef =
    ClassDef::new("CGameDetailsScene", Some("XuiScene"), &[]);

/// Registered at 0x9223ddf0.
pub static C2002_NETWORK_CONFIG_ACTION: ClassDef =
    ClassDef::new("C2002_NetworkConfigAction", Some("DashScene"), &[]);

/// Registered at 0x9223e008.
pub static C2012_TEST_INFORMATION: ClassDef =
    ClassDef::new("C2012_TestInformation", Some("XuiScene"), &[]);

/// Registered at 0x9223e220.
pub static C2013_WINDOWS_CONNECT_NOW: ClassDef =
    ClassDef::new("C2013_WindowsConnectNow", Some("XuiScene"), &[]);

/// Registered at 0x9223e2b8.
pub static C2016_EDIT_IP_SETTINGS: ClassDef =
    ClassDef::new("C2016_EditIPSettings", Some("DashScene"), &[]);

/// Registered at 0x9223e350.
pub static C2018_IP_SETTINGS: ClassDef = ClassDef::new("C2018_IPSettings", Some("DashScene"), &[]);

/// Registered at 0x9223e3e8.
pub static C2033_DNS_CONFIG: ClassDef = ClassDef::new("C2033_DNSConfig", Some("DashScene"), &[]);

/// Registered at 0x9223e480.
pub static C2034_DNS_SETTINGS: ClassDef =
    ClassDef::new("C2034_DNSSettings", Some("DashScene"), &[]);

/// Registered at 0x9223e518.
pub static C2036_PP_PO_E_SETTINGS: ClassDef =
    ClassDef::new("C2036_PPPoESettings", Some("DashScene"), &[]);

/// Registered at 0x9223e5b0.
pub static C2038_CONSOLE_INFORMATION: ClassDef =
    ClassDef::new("C2038_ConsoleInformation", Some("DashScene"), &[]);

/// Registered at 0x9223e648.
pub static C_NETWORK_LIST_ITEM: ClassDef = ClassDef::new("CNetworkListItem", Some("XuiList"), &[]);

/// Registered at 0x9223e6e0.
pub static C2020_AVAILABLE_WIRELESS_NETWORKS: ClassDef =
    ClassDef::new("C2020_AvailableWirelessNetworks", Some("XuiScene"), &[]);

/// Registered at 0x9223e778.
pub static C2022_WIRELESS_SECURITY_SETTINGS: ClassDef =
    ClassDef::new("C2022_WirelessSecuritySettings", Some("DashScene"), &[]);

/// Registered at 0x9223e810.
pub static C2024_ENTER_SETTINGS: ClassDef =
    ClassDef::new("C2024_EnterSettings", Some("DashScene"), &[]);

/// Registered at 0x9223e8a8.
pub static C2026_CHANNEL_LIST: ClassDef =
    ClassDef::new("C2026_ChannelList", Some("DashScene"), &[]);

/// Registered at 0x9223e940.
pub static C2028_WIRELESS_MODES: ClassDef =
    ClassDef::new("C2028_WirelessModes", Some("DashScene"), &[]);

/// Registered at 0x9223e9d8.
pub static C2015_WIRELESS_INFORMATION: ClassDef =
    ClassDef::new("C2015_WirelessInformation", Some("XuiScene"), &[]);

/// Registered at 0x9223eaf0.
pub static C_CONN_STATUS: ClassDef = ClassDef::new("CConnStatus", Some("XuiScene"), &[]);

/// Registered at 0x9223eb88.
pub static C4_LINE_BTN: ClassDef = ClassDef::new("C4LineBtn", Some("XuiNavButton"), &[]);

/// Registered at 0x9223ec20.
pub static C2004_MASTER_TAB: ClassDef = ClassDef::new("C2004_MasterTab", Some("XuiScene"), &[]);

/// Registered at 0x9223ecb8.
pub static C2004_NETWORK_DETAILS: ClassDef =
    ClassDef::new("C2004_NetworkDetails", Some("XuiScene"), &[]);

/// Registered at 0x9223ed50.
pub static C2005_ADDITIONAL_SETTINGS: ClassDef =
    ClassDef::new("C2005_AdditionalSettings", Some("XuiScene"), &[]);

/// Registered at 0x9223ede8.
pub static C2010_TESTING_NETWORK: ClassDef =
    ClassDef::new("C2010_TestingNetwork", Some("XuiScene"), &[]);

/// Registered at 0x9223ee80.
pub static C_TEST_PROFILE_ITEM: ClassDef = ClassDef::new("CTestProfileItem", Some("XuiScene"), &[]);

/// Registered at 0x9223ef18.
pub static C2011_TESTING_LAN: ClassDef = ClassDef::new("C2011_TestingLAN", Some("XuiScene"), &[]);

/// Registered at 0x9223efb0.
pub static STATUS_SCENE: ClassDef = ClassDef::new("StatusScene", Some("XuiScene"), &[]);

static XUI_BOTD_OFFLINE_SCENE_PROPS: [PropDef; 1] = [PropDef::new(
    "ButtonVisual",
    PropType::String,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::String(None)),
)];

/// Registered at 0x922577a8.
pub static XUI_BOTD_OFFLINE_SCENE: ClassDef = ClassDef::new(
    "XuiBOTDOfflineScene",
    Some("XuiScene"),
    &XUI_BOTD_OFFLINE_SCENE_PROPS,
);

static XUI_BOTD_OFFLINE_CONTAINER_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Area",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "DefaultBanner",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
];

/// Registered at 0x92257840.
pub static XUI_BOTD_OFFLINE_CONTAINER: ClassDef = ClassDef::new(
    "XuiBOTDOfflineContainer",
    Some("XuiScene"),
    &XUI_BOTD_OFFLINE_CONTAINER_PROPS,
);

/// Registered at 0x92259b60.
pub static DEVICE_SELECTOR_LIST: ClassDef =
    ClassDef::new("DeviceSelectorList", Some("XuiList"), &[]);

/// Registered at 0x92259bf8.
pub static DEVICE_SELECTOR_SCENE: ClassDef =
    ClassDef::new("DeviceSelectorScene", Some("XuiScene"), &[]);

/// Registered at 0x92259c90.
pub static DEVICE_DETAILS_SCENE: ClassDef =
    ClassDef::new("DeviceDetailsScene", Some("XuiScene"), &[]);

/// Registered at 0x92259d28.
pub static CATEGORIES_LIST: ClassDef = ClassDef::new("CategoriesList", Some("XuiCommonList"), &[]);

/// Registered at 0x92259dc0.
pub static CATEGORIES_SCENE: ClassDef = ClassDef::new("CategoriesScene", Some("XuiScene"), &[]);

/// Registered at 0x92259e58.
pub static ITEMS_LIST: ClassDef = ClassDef::new("ItemsList", Some("XuiList"), &[]);

/// Registered at 0x92259ef0.
pub static ITEMS_SCENE: ClassDef = ClassDef::new("ItemsScene", Some("XuiScene"), &[]);

/// Registered at 0x92259f88.
pub static ITEM_DETAILS_SCENE: ClassDef = ClassDef::new("ItemDetailsScene", Some("XuiScene"), &[]);

/// Registered at 0x9225a020.
pub static OPERATION_PROGRESS_SCENE: ClassDef =
    ClassDef::new("OperationProgressScene", Some("XuiScene"), &[]);

/// Registered at 0x9225a0b8.
pub static FORMAT_HD_SCENE: ClassDef = ClassDef::new("FormatHDScene", Some("XuiScene"), &[]);

/// Registered at 0x9225a150.
pub static DELETE_MUSIC_SCENE: ClassDef = ClassDef::new("DeleteMusicScene", Some("XuiScene"), &[]);

/// Registered at 0x9225a1e8.
pub static HDDVD_DEVICE_SCENE: ClassDef = ClassDef::new("HDDVDDeviceScene", Some("XuiScene"), &[]);

/// Registered at 0x9225a280.
pub static HDDVD_CONTENTS_LIST: ClassDef = ClassDef::new("HDDVDContentsList", Some("XuiList"), &[]);

/// Registered at 0x9225a318.
pub static HDDVD_CONTENTS_SCENE: ClassDef =
    ClassDef::new("HDDVDContentsScene", Some("XuiScene"), &[]);

/// Registered at 0x9225a3b0.
pub static HDDVD_CONTENT_DETAILS_SCENE: ClassDef =
    ClassDef::new("HDDVDContentDetailsScene", Some("XuiScene"), &[]);

/// Registered at 0x92266970.
pub static DOWNLOAD_LIVE_SIGNUP_SCENE: ClassDef =
    ClassDef::new("DownloadLiveSignupScene", Some("XuiScene"), &[]);

/// Registered at 0x92266ad8.
pub static CONGRATULATIONS_SCENE: ClassDef =
    ClassDef::new("CongratulationsScene", Some("DashScene"), &[]);

/// Registered at 0x922686a8.
pub static CI420_VIDEO_STREAM_XUI_CONTROL: ClassDef =
    ClassDef::new("CI420VideoStreamXuiControl", Some("XuiControl"), &[]);

/// Registered at 0x922687c0.
pub static VIDEO_FEED_ZOOM_CONTROL: ClassDef =
    ClassDef::new("VideoFeedZoomControl", Some("XuiScene"), &[]);

/// Registered at 0x92268858.
pub static VIDEO_CHAT_MAIN_SCENE: ClassDef =
    ClassDef::new("VideoChatMainScene", Some("XuiScene"), &[]);

/// Registered at 0x9226fe88.
pub static DASH_IPTV_SETTINGS_SCENE: ClassDef =
    ClassDef::new("DashIPTVSettingsScene", Some("DashScene"), &[]);

/// Registered at 0x9226ff20.
pub static DVR_STORAGE_SCENE: ClassDef = ClassDef::new("DvrStorageScene", Some("XuiScene"), &[]);

/// Registered at 0x9226ffb8.
pub static UNINSTALL_IPTV_SCENE: ClassDef =
    ClassDef::new("UninstallIPTVScene", Some("DashScene"), &[]);

/// Registered at 0x92270050.
pub static CONSOLE_SERIAL_NUM_EDIT: ClassDef =
    ClassDef::new("ConsoleSerialNumEdit", Some("XuiNavButton"), &[]);

static MEDIA_TWIST_PROPS: [PropDef; 1] = [PropDef::new(
    "Active",
    PropType::Integer,
    PropFlags::from_bits(0x8),
    Some(DefaultValue::Integer(0)),
)];

/// Registered at 0x92272ce0.
pub static MEDIA_TWIST: ClassDef =
    ClassDef::new("MediaTwist", Some("XuiControl"), &MEDIA_TWIST_PROPS);

/// Registered at 0x92274968.
pub static ASYNC_TASK_MANAGER0: ClassDef =
    ClassDef::new("AsyncTaskManager0", Some("XuiElement"), &[]);

/// Registered at 0x9227f6c0.
pub static BASE_SCENE: ClassDef = ClassDef::new("BaseScene", Some("XuiScene"), &[]);

/// Registered at 0x9227f758.
pub static EXTENDED_SCENE: ClassDef = ClassDef::new("ExtendedScene", Some("XuiScene"), &[]);

/// Registered at 0x9227faf0.
pub static REPUTATION_SCENE: ClassDef = ClassDef::new("ReputationScene", Some("XuiScene"), &[]);

static XUI_GAMER_CARD_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Format",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(Some("Normal"))),
    ),
    PropDef::new(
        "ShowExtendedPanel",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(true)),
    ),
];

/// Registered at 0x9227fef0.
pub static XUI_GAMER_CARD: ClassDef =
    ClassDef::new("XuiGamerCard", Some("XuiControl"), &XUI_GAMER_CARD_PROPS);

/// Registered at 0x9227ff88.
pub static GAMER_CARD_SCENE: ClassDef = ClassDef::new("GamerCardScene", Some("XuiScene"), &[]);

/// Registered at 0x92280020.
pub static ME_GAMERCARD_WORKER: ClassDef =
    ClassDef::new("MeGamercardWorker", Some("XuiControl"), &[]);

/// Registered at 0x922800b8.
pub static YOU_GAMERCARD_WORKER: ClassDef =
    ClassDef::new("YouGamercardWorker", Some("XuiControl"), &[]);

/// Registered at 0x92287198.
pub static ERROR_SCENE: ClassDef = ClassDef::new("ErrorScene", Some("XuiScene"), &[]);

/// Registered at 0x922872f0.
pub static PC_SOFTWARE_SCENE: ClassDef = ClassDef::new("PcSoftwareScene", Some("XuiScene"), &[]);

/// Registered at 0x92287438.
pub static WELCOME_SCENE: ClassDef = ClassDef::new("WelcomeScene", Some("XuiScene"), &[]);

/// Registered at 0x92287580.
pub static NO_NETWORK_SCENE: ClassDef = ClassDef::new("NoNetworkScene", Some("XuiScene"), &[]);

/// Registered at 0x92287618.
pub static SLOW_NETWORK_SCENE: ClassDef = ClassDef::new("SlowNetworkScene", Some("XuiScene"), &[]);

/// Registered at 0x922876b0.
pub static RECONFIGURE_WIRELESS_SCENE: ClassDef =
    ClassDef::new("ReconfigureWirelessScene", Some("XuiScene"), &[]);

/// Registered at 0x92287748.
pub static WIRELESS80211B_SCENE: ClassDef =
    ClassDef::new("Wireless80211bScene", Some("XuiScene"), &[]);

/// Registered at 0x922877e0.
pub static LIMITED_NETWORK_SCENE: ClassDef =
    ClassDef::new("LimitedNetworkScene", Some("XuiScene"), &[]);

/// Registered at 0x92287878.
pub static NETWORK_SETTINGS_SCENE: ClassDef =
    ClassDef::new("NetworkSettingsScene", Some("XuiScene"), &[]);

/// Registered at 0x92287910.
pub static MEDIA_CENTER_SCENE: ClassDef = ClassDef::new("MediaCenterScene", Some("XuiScene"), &[]);

/// Registered at 0x92287a58.
pub static CONTACT_ERROR_SCENE: ClassDef =
    ClassDef::new("ContactErrorScene", Some("XuiScene"), &[]);

/// Registered at 0x92287af0.
pub static OLD_VERSION_SCENE: ClassDef = ClassDef::new("OldVersionScene", Some("XuiScene"), &[]);

/// Registered at 0x92287b88.
pub static WAITING_SCENE: ClassDef = ClassDef::new("WaitingScene", Some("XuiScene"), &[]);

/// Registered at 0x92287d88.
pub static IPTV_CONNECTING_SCENE: ClassDef =
    ClassDef::new("IptvConnectingScene", Some("XuiScene"), &[]);

/// Registered at 0x9228ec30.
pub static DASH_LIVE_SCENE: ClassDef = ClassDef::new("DashLiveScene", Some("XuiScene"), &[]);

/// Registered at 0x9228edf0.
pub static DASH_LIVE_SIGNED_IN: ClassDef = ClassDef::new("DashLiveSignedIn", Some("XuiScene"), &[]);

/// Registered at 0x9228ee88.
pub static DASH_LIVE_SIGNED_OUT: ClassDef =
    ClassDef::new("DashLiveSignedOut", Some("XuiScene"), &[]);

/// Registered at 0x9228f470.
pub static DASH_MP_SCENE: ClassDef = ClassDef::new("DashMPScene", Some("XuiScene"), &[]);

/// Registered at 0x922938d0.
pub static C_GAMES_BLADE_SCENE: ClassDef = ClassDef::new("CGamesBladeScene", Some("XuiScene"), &[]);

/// Registered at 0x92293a98.
pub static C_GAMES_BLADE_SIGNED_IN: ClassDef =
    ClassDef::new("CGamesBladeSignedIn", Some("DashScene"), &[]);

/// Registered at 0x92293b30.
pub static C_GAMES_BLADE_SIGNED_OUT: ClassDef =
    ClassDef::new("CGamesBladeSignedOut", Some("DashScene"), &[]);

/// Registered at 0x92293c48.
pub static C_GAMES_BLADE_NAV_BUTTON: ClassDef =
    ClassDef::new("CGamesBladeNavButton", Some("XuiNavButton"), &[]);

/// Registered at 0x922945a8.
pub static DASH_MEDIA_SCENE: ClassDef = ClassDef::new("DashMediaScene", Some("XuiScene"), &[]);

/// Registered at 0x92294768.
pub static MEDIA_BLADE_SIGNED_IN_SCENE: ClassDef =
    ClassDef::new("MediaBladeSignedInScene", Some("DashScene"), &[]);

/// Registered at 0x92294800.
pub static MEDIA_BLADE_SIGNED_OUT_SCENE: ClassDef =
    ClassDef::new("MediaBladeSignedOutScene", Some("DashScene"), &[]);

/// Registered at 0x92294910.
pub static MEDIA_BLADE_META_IPTV: ClassDef =
    ClassDef::new("MediaBladeMetaIptv", Some("XuiScene"), &[]);

/// Registered at 0x922959e8.
pub static OOBE_SCENE: ClassDef = ClassDef::new("OOBEScene", Some("XuiScene"), &[]);

/// Registered at 0x92297cd8.
pub static OOBE_COUNTRY: ClassDef = ClassDef::new("oobeCountry", Some("DashScene"), &[]);

/// Registered at 0x92297d70.
pub static OOBE_LANGUAGE: ClassDef = ClassDef::new("oobeLanguage", Some("DashScene"), &[]);

/// Registered at 0x92298720.
pub static C_GAMER_TILE: ClassDef = ClassDef::new("CGamerTile", Some("XuiScene"), &[]);

/// Registered at 0x92298de0.
pub static C_RESOLUTION_CHOICE: ClassDef =
    ClassDef::new("CResolutionChoice", Some("DashScene"), &[]);

/// Registered at 0x92298e78.
pub static C_PAL_CHOICE: ClassDef = ClassDef::new("CPalChoice", Some("DashScene"), &[]);

/// Registered at 0x92298f10.
pub static C_STORAGE_DEVICE: ClassDef = ClassDef::new("CStorageDevice", Some("XuiScene"), &[]);

/// Registered at 0x92298fa8.
pub static OOBE_PROFILE_CREATION: ClassDef =
    ClassDef::new("oobeProfileCreation", Some("XuiScene"), &[]);

/// Registered at 0x92299040.
pub static C_SETUP_COMPLETE: ClassDef = ClassDef::new("CSetupComplete", Some("DashScene"), &[]);

/// Registered at 0x922990d8.
pub static C_WELCOME_VIDEO_SCENE: ClassDef =
    ClassDef::new("CWelcomeVideoScene", Some("DashScene"), &[]);

/// Registered at 0x922991f0.
pub static C_PROFILE_CHOICE: ClassDef = ClassDef::new("CProfileChoice", Some("DashScene"), &[]);

/// Registered at 0x92299288.
pub static GAMER_TILE_LIST: ClassDef = ClassDef::new("GamerTileList", Some("XuiList"), &[]);

/// Registered at 0x922a0208.
pub static DVD_INFO_PANE: ClassDef = ClassDef::new("DvdInfoPane", Some("XuiScene"), &[]);

/// Registered at 0x922a0490.
pub static XMF_PREBUFFERING_SCENE: ClassDef =
    ClassDef::new("XmfPrebufferingScene", Some("XuiScene"), &[]);

/// Registered at 0x922a1278.
pub static DVD_PAD_DISPLAY: ClassDef = ClassDef::new("DvdPadDisplay", Some("XuiScene"), &[]);

/// Registered at 0x922a1310.
pub static DVD_PAD_PLAYBACK: ClassDef = ClassDef::new("DvdPadPlayback", Some("XuiScene"), &[]);

/// Registered at 0x922a13a8.
pub static DVD_RC_DISPLAY: ClassDef = ClassDef::new("DvdRcDisplay", Some("XuiScene"), &[]);

/// Registered at 0x922a3070.
pub static DVD_HELP_PANE: ClassDef = ClassDef::new("DvdHelpPane", Some("XuiScene"), &[]);

static SCRIPT_DATA_PROPS: [PropDef; 4] = [
    PropDef::new(
        "Query",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "State",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "ItemCount",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "Selected",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x9258c7d8.
pub static SCRIPT_DATA: ClassDef =
    ClassDef::new("ScriptData", Some("XuiElement"), &SCRIPT_DATA_PROPS);

static SCRIPT_SCENE_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Script",
        PropType::String,
        PropFlags::from_bits(0x18),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Visible",
        PropType::Bool,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Bool(true)),
    ),
];

/// Registered at 0x9258c870.
pub static SCRIPT_SCENE: ClassDef =
    ClassDef::new("ScriptScene", Some("XuiScene"), &SCRIPT_SCENE_PROPS);

static SCRIPT_BUTTON_PROPS: [PropDef; 4] = [
    PropDef::new(
        "DataSet",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "DataAssociation",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Embedded",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Item",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(-1)),
    ),
];

/// Registered at 0x9258caa8.
pub static SCRIPT_BUTTON: ClassDef =
    ClassDef::new("ScriptButton", Some("XuiButton"), &SCRIPT_BUTTON_PROPS);

static SCRIPT_LIST_PROPS: [PropDef; 3] = [
    PropDef::new(
        "DataSet",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "DataAssociation",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Embedded",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
];

/// Registered at 0x9258cb40.
pub static SCRIPT_LIST: ClassDef = ClassDef::new("ScriptList", Some("XuiList"), &SCRIPT_LIST_PROPS);

static SCRIPT_IMAGE_PROPS: [PropDef; 4] = [
    PropDef::new(
        "DataSet",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "DataAssociation",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Embedded",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Item",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(-1)),
    ),
];

/// Registered at 0x9258cbd8.
pub static SCRIPT_IMAGE: ClassDef =
    ClassDef::new("ScriptImage", Some("XuiControl"), &SCRIPT_IMAGE_PROPS);
