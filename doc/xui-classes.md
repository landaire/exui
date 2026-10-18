# XUI class registry

Classes registered in dash 6717, their base classes and property definitions. Every table here was produced by emulating the registration functions of the binary and reading the definition records they build; nothing is taken from other sources.

## Components

| Address | Name | Role | Status |
| --- | --- | --- | --- |
| 0x92147868 | `XuiRegisterClass` | Registers a class from an `XUIClass` record. | complete |
| 0x92144210 | `XuiFindClass` | Class handle by name, or 0. | complete |
| 0x921441a0 | `XuiClassTableLookup` | Locked lookup in the class hash table at 0x92809f28. | complete |
| 0x9213da98 | `XuiStringHashFind` | Hash `h = h*33 + unit`, 27 buckets, `wcscmp` chain compare. | complete |
| 0x9213ce00 | `XuiClassGetBaseClass` | Handle +0x28. | complete |
| 0x9213d1d0 | `XuiClassGetPropDefs` | Handle +0x1C and +0x20; S_OK with 0 entries when the class has none. | complete |
| 0x9214b368 | `XuiInit` | Stores `XUIInitParams` +4 and the resolver (+8), registers the 41 built-in classes. | partial |
| 0x9214a608 | `XuiUninit` | Clears the registry state and the resolver. | partial |
| 0x9214c668 | `XuiSoundXAudioRegister` | Registers `XuiSoundXAudio`. | complete |
| 0x92199a20 | `XuiHtmlRegister` | Registers the three HTML classes. | complete |
| 0x92809cd4 | `g_XuiResolver` | Resolver interface; method 1 is asked for unknown class names (see xui-format.md). | complete |

Per class there is a registration function `XuiRegisterClass_<Class>` and, for classes with properties, a getter `<Class>_GetPropDefs` that builds the definition table on first use (guarded static) and returns it with its count. Compound properties have their own getters `<Class>_<Property>_GetPropDefs`. All are named in the database.

## Records

`XUIClass`, passed to `XuiRegisterClass` (0x28 bytes):

| Offset | Meaning |
| --- | --- |
| 0x00 | 0 in every call. |
| 0x04 | Class name. Required, non-empty. |
| 0x08 | Base class name, or null for `XuiElement`. |
| 0x0C | Required-children description; only `XuiMessageBox` sets it. |
| 0x10, 0x14, 0x18 | Three function pointers, all required. |
| 0x1C | Property definition table, or null. |
| 0x20 | Property count. |
| 0x24 | Copied to the handle; 0 in every call. |

`XuiRegisterClass` fails with E_INVALIDARG on a null record, output pointer, name or function pointer, with 0x80300005 when the name is already registered and with 0x80300006 when the base class is not registered yet. Registration order therefore matters, and a second registration of a name fails without changing the first. The class handle (0x38 bytes) copies the strings and functions, holds the base handle at +0x28, and at +0x2C the first global property id. Registration overwrites each top-level definition's +0x4 field with a running global property id; compound sub-definitions keep their local index.

Property definition (0x30 bytes):

| Offset | Meaning |
| --- | --- |
| 0x00 | Flags. Bit 0: indexed, the loader reads an element count. Bits 1-4 occur (values below) but do not affect loading; their meaning is not known yet. |
| 0x04 | Index, replaced by a global id at registration. |
| 0x08 | Field offset in the element object. |
| 0x0C | Bit mask for packed bool fields. |
| 0x10 | Name. |
| 0x14 | Type code (1 Bool, 2 Integer, 3 Unsigned, 4 Float, 5 String, 6 Color, 7 Vector, 8 Quaternion, 9 Compound, 10 Custom). |
| 0x18 | Default value (`XUIValue*`), or null. |
| 0x1C | Custom type handler table (only `XuiFigure.Points`, 0x9279ac38). |
| 0x20 | Getter override. |
| 0x24 | Setter override. |
| 0x28 | Compound sub-definition getter. |
| 0x2C | Element count getter for indexed properties. |

`XUIValue` (0x18 bytes): u32 type, 16 bytes of payload (bool, integer, float, color or string pointer in the first word; vectors and quaternions inline), u32 at +0x14.

Bit positions in a property block are the definition's position in the table, which is the order of the tables below.

## Registration groups

`XuiInit` registers, in order:

`XuiElement`, `XuiCanvas`, `XuiFigure`, `XuiControl`, `XuiScene`, `XuiButton`, `XuiBackButton`, `XuiNavButton`, `XuiCheckbox`, `XuiRadioButton`, `XuiRadioGroup`, `XuiText`, `XuiTextPresenter`, `XuiVisual`, `XuiGroup`, `XuiSound`, `XuiTransition`, `XuiListItem`, `XuiScrollEnd`, `XuiScrollBar`, `XuiList`, `XuiCommonList`, `XuiProgressBar`, `XuiSlider`, `XuiCaret`, `XuiEdit`, `XuiImagePresenter`, `XuiImage`, `XuiMessageBox`, `XuiLabel`, `XuiTabScene`, `XuiNineGrid`, `XuiEffect`, `XuiTextureEffect`, `XuiGrayscaleEffect`, `XuiRecolorEffect`, `XuiBrightPassEffect`, `XuiEdgeEffect`, `XuiHVBlurEffect`, `XuiHBlurEffect`, `XuiVBlurEffect`.

The dashboard calls `XuiSoundXAudioRegister` and `XuiHtmlRegister` from 0x9210c7a0 (and 0x922a3108). Its own classes are registered by group functions, listed by caller. Several groups run on demand when a blade or feature initializes, so the overall runtime order is not fixed; every base below is either a built-in or registered earlier in the same chain.

| Group function | Classes |
| --- | --- |
| 0x9210c488 | `LiveVisionControl` |
| 0x921128a8 | `LiveVisionDashEffect`, `DashMainScene`, `DVDTrayScene`, `TraySceneButton`, `TraySceneLoader`, `CTitleMetaDataScene`, `CXuiEditAutoScroll`, `MediaPackage`, `CLiveMessageBox`, `MetaPanelScene`, `DashScene`, `CWaitForLogonScene`, `NetworkDeviceSelectScene`, `NetworkDeviceMetaPanelScene`, `NetworkDeviceManagementScene`, `DashSystemScene`, `Wallpaper`, `dashSignOut`, `CDashListChooser` |
| 0x9214c668 | `XuiSoundXAudio` |
| 0x92199a20 | `XuiHtmlElement`, `XuiHtmlPresenter`, `XuiHtmlControl` |
| 0x921ac478 | `MediaSourceNoComputersMetapanelScene`, `MediaSourceList` |
| 0x921b0258 | `DashVideo` |
| 0x921b64c8 | `DashDataList`, `DashData` |
| 0x921ba2b0 | `CBladeMOTD`, `DashBladeTab` |
| 0x921c48c0 | `dashSettingsRoot`, `dashCValueSpin`, `dashCDate`, `dashCTimezone`, `dashCLanguage`, `dashCCountry`, `dashCRatingView`, `dashCScreenSize`, `dashCHints`, `dashRemoteCList`, `DashSettingsList`, `dashPControlSelect`, `dashPControlSettings_Video`, `dashPControlSettings_VideoMovie`, `dashPControlSettings_VideoTV`, `dashPControlSettings_VideoUnrated`, `dashPControlSettings_LiveA`, `dashPControlSettings_Content`, `dashPControlSettings_LiveC`, `dashPControlSettings_Passcode`, `dashPControlSettings_PasscodeHint`, `dashPasscodeEdit`, `dashPasscodeNEdit`, `CFamilyTimerDurationList`, `CFamilyTimerSettingsScene`, `dashCTextSpin`, `dashCTime`, `dashCCInputScene`, `dashVideoSettings`, `dashVideoSettings_HD`, `dashVideoSettings_Format`, `dashVideoSettings_60`, `dashAudioSettings_D`, `dashAudioSettings_FX`, `dashClockSettings_Menu`, `dashClockSettings`, `dashClockSettings_TZone`, `dashClockSettings_1224`, `dashCountrySettings`, `dashAutoOff`, `dashRemoteC`, `dashScreensaver`, `dashOutputLevels`, `dashSystemReset`, `dashStartUp`, `dashLiveVisionSettingsScene`, `dashBackgroundDownloads`, `dashShutdownSettings`, `dashPControlSettingsMenu`, `dashPControlSettings_Game`, `dashPControlSettings_VideoExplicit`, `dashAudioSettings`, `dashLanguageSettings`, `dashClockSettings_DSaving` |
| 0x921ce9f8 | `MediaSourceSelectionScene` |
| 0x921ced28 | `GridList`, `TransportControlsScene`, `ImageRenderControl`, `DeviceMainScene`, `FullScreenPhotoScene` |
| 0x921da040 | `SongList`, `MediaContainerList`, `MusicPlayPauseButton`, `GotoMusicPlayerScene`, `GotoMusicPlayerDashScene`, `SongListScene`, `MediaContainerListScene`, `MediaContainerOptionsScene`, `ArtistOptionsScene`, `SongOptionsScene`, `MusicLibraryScene`, `EditNowPlayingQueueScene`, `EditNowPlayingQueueList`, `RipCDProgressList`, `EditBoxButton`, `NowPlayingScene`, `MusicEqualizer`, `NowPlayingList`, `FileViewScene`, `FileViewList`, `MusicPlaybackModeButton`, `DeviceNotBrowseableScene`, `SongMetadataScene`, `RipCDProgressScene`, `RipCDSelectScene`, `EditTagInfoScene`, `NowPlayingRipOrEditButton`, `VisualizerButton`, `VisualizerScene`, `MusicTransportControlsScene` |
| 0x921d97f8 | `MediaSourceSelectionScene` |
| 0x921ed7d8, 0x921ee010 | `MediaSourceSelectionScene` |
| 0x921ee010 | `VideoContentList`, `VideoScene`, `VideoContentScene`, `VideoData`, `VideoCategoriesList`, `VideosDetailsTwist`, `VideosTwistFilter` |
| 0x921fd088 | `CPostalDataList`, `CAddressButton`, `CStateList`, `CContactAddressScene`, `CChangeLiveIDCurrentSignIn`, `CChangeLiveIDNewSignIn`, `CChangeLiveIDParentSignIn`, `CPrivTwoOptionsScene`, `CPrivThreeOptionsScene`, `CLiveKeyboard`, `CAMHomeScene`, `CAMMyMembershipsScene`, `CMyMembershipsList`, `CMyMembershipsScene`, `CPassportSignInButton`, `CPassportSignInScene`, `CPassportPasswordButton`, `CPassportPasswordScene`, `CWLIDMenuScene`, `CChangeLiveIDMenuScene`, `CChangeLiveIDFinishScene`, `CChangeLiveIDCongratsScene`, `PrivacySettingsList`, `CPrivacyMainScene`, `CMarketingTwoOptionsScene`, `CPolicyMainScene`, `CPolicyDisplayScene`, `CPCMainScene`, `CPCPrivFriendsScene`, `CGameplaySettingsScene`, `CPCContentSettingsScene`, `CMoreInfoScene`, `CAboutMSPointsListScene`, `CPostalDataListScene`, `CStateListScene` |
| 0x9220e870 | `CMessengerWelcomeScene`, `CMessengerSelectWLIDScene`, `CMessengerSignupCompleteScene`, `CFriendRequestRecipientsList`, `CFriendRequestIntroScene`, `CFriendRequestMainScene`, `FriendRequestProgressScene`, `EditButton`, `PasswordEditButton` |
| 0x92215388 | `Arcade::CCategoryScene`, `Arcade::CFriendImageCtrl`, `Arcade::CTitleScene::CGameScene`, `Arcade::CTitleScene::CAchievementScene`, `Arcade::CTitleScene::CLeaderboardScene`, `Arcade::CTitleScene::CPresenceScene`, `Arcade::CTitleScene`, `Arcade::CTitleAchievementsScene`, `Arcade::CCompareAchievementsScene`, `Arcade::CTitleLeaderboardScene`, `Arcade::CTellAFriendScene`, `Arcade::CTitleLaunchScene`, `Arcade::CTitleSelectorScene`, `Arcade::CCategorySelectorScene`, `Arcade::CEZPassScene`, `Arcade::CFriendsPlayingNowScene`, `Arcade::CHomepageScene`, `Arcade::CDownloadMetaScene`, `Arcade::CMyArcadeMetaScene`, `Arcade::CFriendsMetaScene`, `Arcade::CTwistSelectorScene`, `ArcadeTwistFilter`, `Arcade::CRecentGameMetaScene`, `Arcade::CCategoryListCtrl`, `Arcade::CTitleListCtrl`, `Arcade::CLeaderboardListCtrl`, `Arcade::CTellAFriendListCtrl`, `Arcade::CFriendsListCtrl`, `Arcade::CMultiTextNavButton`, `Arcade::CAchievementListCtrl` |
| 0x922388f8 | `CGameShowcaseListList`, `CGameShowcaseScene`, `CAchievementsGridTitleList`, `CAchievementsGridList`, `CAchievementsGridScene`, `CGameDetailsScene` |
| 0x9223f048 | `C2002_NetworkConfigAction`, `C2012_TestInformation`, `C2013_WindowsConnectNow`, `C2016_EditIPSettings`, `C2018_IPSettings`, `C2033_DNSConfig`, `C2034_DNSSettings`, `C2036_PPPoESettings`, `C2038_ConsoleInformation`, `CNetworkListItem`, `C2020_AvailableWirelessNetworks`, `C2022_WirelessSecuritySettings`, `C2024_EnterSettings`, `C2026_ChannelList`, `C2028_WirelessModes`, `C2015_WirelessInformation`, `CConnStatus`, `C4LineBtn`, `C2004_MasterTab`, `C2004_NetworkDetails`, `C2005_AdditionalSettings`, `C2010_TestingNetwork`, `CTestProfileItem`, `C2011_TestingLAN`, `StatusScene` |
| 0x922578d8 | `XuiBOTDOfflineScene`, `XuiBOTDOfflineContainer` |
| 0x9225a448 | `DeviceSelectorList`, `DeviceSelectorScene`, `DeviceDetailsScene`, `CategoriesList`, `CategoriesScene`, `ItemsList`, `ItemsScene`, `ItemDetailsScene`, `OperationProgressScene`, `FormatHDScene`, `DeleteMusicScene`, `HDDVDDeviceScene`, `HDDVDContentsList`, `HDDVDContentsScene`, `HDDVDContentDetailsScene` |
| 0x92266b70 | `DownloadLiveSignupScene`, `CongratulationsScene` |
| 0x922688f0 | `CI420VideoStreamXuiControl`, `VideoFeedZoomControl`, `VideoChatMainScene` |
| 0x922700e8 | `DashIPTVSettingsScene`, `DvrStorageScene`, `UninstallIPTVScene`, `ConsoleSerialNumEdit` |
| 0x92272d78 | `MediaTwist` |
| 0x92274a00 | `AsyncTaskManager0` |
| 0x92280150 | `BaseScene`, `ExtendedScene`, `ReputationScene`, `XuiGamerCard`, `GamerCardScene`, `MeGamercardWorker`, `YouGamercardWorker` |
| 0x92287c20 | `ErrorScene`, `PcSoftwareScene`, `WelcomeScene`, `NoNetworkScene`, `SlowNetworkScene`, `ReconfigureWirelessScene`, `Wireless80211bScene`, `LimitedNetworkScene`, `NetworkSettingsScene`, `MediaCenterScene`, `ContactErrorScene`, `OldVersionScene`, `WaitingScene` |
| 0x92287e20 | `IptvConnectingScene` |
| 0x9228ef20 | `DashLiveScene`, `DashLiveSignedIn`, `DashLiveSignedOut` |
| 0x9228f508 | `DashMPScene` |
| 0x92293ce0 | `CGamesBladeScene`, `CGamesBladeSignedIn`, `CGamesBladeSignedOut`, `CGamesBladeNavButton` |
| 0x922949a8 | `DashMediaScene`, `MediaBladeSignedInScene`, `MediaBladeSignedOutScene`, `MediaBladeMetaIptv` |
| 0x92295a80 | `OOBEScene` |
| 0x92297e08 | `oobeCountry`, `oobeLanguage` |
| 0x92299320 | `CGamerTile`, `CResolutionChoice`, `CPalChoice`, `CStorageDevice`, `oobeProfileCreation`, `CSetupComplete`, `CWelcomeVideoScene`, `CProfileChoice`, `GamerTileList` |
| 0x922a3108 | `DvdInfoPane`, `XmfPrebufferingScene`, `DvdPadDisplay`, `DvdPadPlayback`, `DvdRcDisplay`, `DvdHelpPane` |
| 0x9258cc70 | `ScriptData`, `ScriptScene`, `ScriptButton`, `ScriptList`, `ScriptImage` |

`MediaSourceSelectionScene` is registered by three functions (0x921ce698, 0x921d9760, 0x921ed540) with the same base and no properties but different function pointers. Only the first to run succeeds.

## Class tree

- XuiElement
  - AsyncTaskManager0
  - CLiveKeyboard
  - CLiveMessageBox
  - DashVideo
  - ScriptData
    - DashData
      - VideoData
  - XuiCanvas
  - XuiControl
    - Arcade::CFriendImageCtrl
    - CI420VideoStreamXuiControl
    - ImageRenderControl
    - LiveVisionControl
    - LiveVisionDashEffect
    - MeGamercardWorker
    - MediaTwist
      - ArcadeTwistFilter
      - VideosDetailsTwist
      - VideosTwistFilter
    - ScriptImage
    - Wallpaper
    - XuiButton
      - CAddressButton
      - CPassportPasswordButton
      - CPassportSignInButton
      - EditButton
      - MusicPlayPauseButton
      - MusicPlaybackModeButton
      - NowPlayingRipOrEditButton
      - PasswordEditButton
      - ScriptButton
      - TraySceneButton
      - VisualizerButton
      - XuiBackButton
      - XuiNavButton
        - Arcade::CMultiTextNavButton
        - C4LineBtn
        - CGamesBladeNavButton
        - ConsoleSerialNumEdit
        - EditBoxButton
        - dashPasscodeNEdit
        - dashSignOut
      - dashPasscodeEdit
    - XuiCaret
    - XuiCheckbox
      - XuiListItem
    - XuiEdit
      - CXuiEditAutoScroll
    - XuiGamerCard
    - XuiHtmlControl
    - XuiLabel
      - MusicEqualizer
    - XuiList
      - Arcade::CAchievementListCtrl
      - Arcade::CCategoryListCtrl
      - Arcade::CFriendsListCtrl
      - Arcade::CLeaderboardListCtrl
      - Arcade::CTellAFriendListCtrl
      - Arcade::CTitleListCtrl
      - CAchievementsGridList
      - CAchievementsGridTitleList
      - CFamilyTimerDurationList
      - CGameShowcaseListList
      - CMyMembershipsList
      - CNetworkListItem
      - CPostalDataList
      - CStateList
      - DeviceSelectorList
      - EditNowPlayingQueueList
      - FileViewList
      - GamerTileList
      - GridList
      - HDDVDContentsList
      - ItemsList
      - MediaContainerList
      - MediaSourceList
      - NowPlayingList
      - RipCDProgressList
      - ScriptList
        - DashDataList
          - VideoCategoriesList
          - VideoContentList
      - SongList
      - XuiCommonList
        - CDashListChooser
        - CFriendRequestRecipientsList
        - CategoriesList
        - DashSettingsList
        - PrivacySettingsList
      - dashCCountry
      - dashCHints
      - dashCLanguage
      - dashCRatingView
      - dashCScreenSize
      - dashCTextSpin
      - dashCTimezone
      - dashCValueSpin
      - dashRemoteCList
    - XuiProgressBar
    - XuiRadioButton
    - XuiRadioGroup
    - XuiScene
      - Arcade::CCategoryScene
      - Arcade::CCompareAchievementsScene
      - Arcade::CDownloadMetaScene
      - Arcade::CFriendsMetaScene
      - Arcade::CFriendsPlayingNowScene
      - Arcade::CMyArcadeMetaScene
      - Arcade::CRecentGameMetaScene
      - Arcade::CTellAFriendScene
      - Arcade::CTitleAchievementsScene
      - Arcade::CTitleLaunchScene
      - Arcade::CTitleLeaderboardScene
      - Arcade::CTitleScene
      - Arcade::CTitleScene::CAchievementScene
      - Arcade::CTitleScene::CGameScene
      - Arcade::CTitleScene::CLeaderboardScene
      - Arcade::CTitleScene::CPresenceScene
      - BaseScene
      - C2004_MasterTab
      - C2004_NetworkDetails
      - C2005_AdditionalSettings
      - C2010_TestingNetwork
      - C2011_TestingLAN
      - C2012_TestInformation
      - C2013_WindowsConnectNow
      - C2015_WirelessInformation
      - C2020_AvailableWirelessNetworks
      - CBladeMOTD
      - CConnStatus
      - CGameDetailsScene
      - CGameShowcaseScene
      - CGamerTile
      - CGamesBladeScene
      - CMyMembershipsScene
      - CStorageDevice
      - CTestProfileItem
      - CTitleMetaDataScene
      - CategoriesScene
      - ContactErrorScene
      - DVDTrayScene
      - DashBladeTab
      - DashLiveScene
      - DashLiveSignedIn
      - DashLiveSignedOut
      - DashMPScene
      - DashMediaScene
      - DashScene
        - Arcade::CCategorySelectorScene
        - Arcade::CEZPassScene
        - Arcade::CHomepageScene
        - Arcade::CTitleSelectorScene
        - Arcade::CTwistSelectorScene
        - C2002_NetworkConfigAction
        - C2016_EditIPSettings
        - C2018_IPSettings
        - C2022_WirelessSecuritySettings
        - C2024_EnterSettings
        - C2026_ChannelList
        - C2028_WirelessModes
        - C2033_DNSConfig
        - C2034_DNSSettings
        - C2036_PPPoESettings
        - C2038_ConsoleInformation
        - CAMHomeScene
        - CAMMyMembershipsScene
        - CAboutMSPointsListScene
        - CAchievementsGridScene
        - CChangeLiveIDCongratsScene
        - CChangeLiveIDCurrentSignIn
        - CChangeLiveIDFinishScene
        - CChangeLiveIDMenuScene
        - CChangeLiveIDNewSignIn
        - CChangeLiveIDParentSignIn
        - CContactAddressScene
        - CFamilyTimerSettingsScene
        - CFriendRequestIntroScene
        - CFriendRequestMainScene
        - CGameplaySettingsScene
        - CGamesBladeSignedIn
        - CGamesBladeSignedOut
        - CMarketingTwoOptionsScene
        - CMessengerSelectWLIDScene
        - CMessengerSignupCompleteScene
        - CMessengerWelcomeScene
        - CMoreInfoScene
        - CPCContentSettingsScene
        - CPCMainScene
        - CPCPrivFriendsScene
        - CPalChoice
        - CPassportPasswordScene
        - CPassportSignInScene
        - CPolicyDisplayScene
        - CPolicyMainScene
        - CPostalDataListScene
        - CPrivThreeOptionsScene
        - CPrivTwoOptionsScene
        - CPrivacyMainScene
        - CProfileChoice
        - CResolutionChoice
        - CSetupComplete
        - CStateListScene
        - CWLIDMenuScene
        - CWaitForLogonScene
        - CWelcomeVideoScene
        - CongratulationsScene
        - DashIPTVSettingsScene
        - DashSystemScene
        - FriendRequestProgressScene
        - GotoMusicPlayerDashScene
          - MusicLibraryScene
        - MediaBladeSignedInScene
        - MediaBladeSignedOutScene
        - MediaSourceSelectionScene
        - NetworkDeviceSelectScene
        - UninstallIPTVScene
        - dashAudioSettings
        - dashClockSettings_Menu
        - dashLiveVisionSettingsScene
        - dashPControlSelect
        - dashPControlSettingsMenu
        - dashPControlSettings_Game
        - dashPControlSettings_Video
        - dashPControlSettings_VideoExplicit
        - dashPControlSettings_VideoMovie
        - dashPControlSettings_VideoTV
        - dashPControlSettings_VideoUnrated
        - dashSettingsRoot
        - dashShutdownSettings
        - dashVideoSettings
        - dashVideoSettings_Format
        - oobeCountry
        - oobeLanguage
      - DeleteMusicScene
      - DeviceDetailsScene
      - DeviceMainScene
      - DeviceSelectorScene
      - DownloadLiveSignupScene
      - DvdHelpPane
      - DvdInfoPane
      - DvdPadDisplay
      - DvdPadPlayback
      - DvdRcDisplay
      - DvrStorageScene
      - EditNowPlayingQueueScene
      - EditTagInfoScene
      - ErrorScene
      - ExtendedScene
      - FormatHDScene
      - FullScreenPhotoScene
      - GamerCardScene
      - GotoMusicPlayerScene
        - ArtistOptionsScene
        - DeviceNotBrowseableScene
        - FileViewScene
        - MediaContainerOptionsScene
        - SongOptionsScene
      - HDDVDContentDetailsScene
      - HDDVDContentsScene
      - HDDVDDeviceScene
      - IptvConnectingScene
      - ItemDetailsScene
      - ItemsScene
      - LimitedNetworkScene
      - MediaBladeMetaIptv
      - MediaCenterScene
      - MediaContainerListScene
      - MediaPackage
      - MediaSourceNoComputersMetapanelScene
      - MetaPanelScene
      - MusicTransportControlsScene
      - NetworkDeviceManagementScene
      - NetworkDeviceMetaPanelScene
      - NetworkSettingsScene
      - NoNetworkScene
      - NowPlayingScene
      - OOBEScene
      - OldVersionScene
      - OperationProgressScene
      - PcSoftwareScene
      - ReconfigureWirelessScene
      - ReputationScene
      - RipCDProgressScene
      - RipCDSelectScene
      - ScriptScene
        - VideoContentScene
      - SlowNetworkScene
      - SongListScene
      - SongMetadataScene
      - StatusScene
      - TransportControlsScene
      - TraySceneLoader
      - VideoChatMainScene
      - VideoFeedZoomControl
      - VideoScene
      - VisualizerScene
      - WaitingScene
      - WelcomeScene
      - Wireless80211bScene
      - XmfPrebufferingScene
      - XuiBOTDOfflineContainer
      - XuiBOTDOfflineScene
      - XuiMessageBox
      - XuiTabScene
        - DashMainScene
      - dashAudioSettings_D
      - dashAudioSettings_FX
      - dashAutoOff
      - dashBackgroundDownloads
      - dashCCInputScene
      - dashCDate
      - dashCTime
      - dashClockSettings
      - dashClockSettings_1224
      - dashClockSettings_DSaving
      - dashClockSettings_TZone
      - dashCountrySettings
      - dashLanguageSettings
      - dashOutputLevels
      - dashPControlSettings_Content
      - dashPControlSettings_LiveA
      - dashPControlSettings_LiveC
      - dashPControlSettings_Passcode
      - dashPControlSettings_PasscodeHint
      - dashRemoteC
      - dashScreensaver
      - dashStartUp
      - dashSystemReset
      - dashVideoSettings_60
      - dashVideoSettings_HD
      - oobeProfileCreation
    - XuiScrollBar
    - XuiScrollEnd
    - XuiSlider
    - YouGamercardWorker
  - XuiEffect
    - XuiBrightPassEffect
    - XuiEdgeEffect
    - XuiGrayscaleEffect
    - XuiHBlurEffect
    - XuiHVBlurEffect
    - XuiRecolorEffect
    - XuiTextureEffect
    - XuiVBlurEffect
  - XuiFigure
  - XuiGroup
  - XuiHtmlElement
  - XuiHtmlPresenter
  - XuiImage
  - XuiImagePresenter
  - XuiNineGrid
  - XuiSound
    - XuiSoundXAudio
  - XuiText
  - XuiTextPresenter
  - XuiTransition
  - XuiVisual

## Properties

Columns: bit position, name (compound members as `Parent.Member`, positions local to the compound), type, flags, default, field offset (+0x08, relative to the class level's data), bool mask (+0x0C), getter (+0x20), setter (+0x24) and element count getter (+0x2C). Classes not listed declare no properties.

### DashScene

Base `XuiScene`. Registered at 0x92112350, getter 0x9210f748, table 0x928040b8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | PanelSettings | String | 0x0 | null | 0x8 | - | - | - | - |
| 1 | PanelStrings | String | 0x4 | null | 0xc | - | - | - | - |
| 2 | PanelScenePaths | String | 0x0 | null | 0x10 | - | - | - | - |

### XuiText

Base `XuiElement`. Registered at 0x92147f28, getter 0x921456e0, table 0x9279a298.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Text | String | 0x4 | null | 0x20 | - | - | - | - |
| 1 | TextColor | Color | 0x0 | 0xff000000 | 0x4 | - | - | - | - |
| 2 | DropShadowColor | Color | 0x0 | 0x80000000 | 0x8 | - | - | - | - |
| 3 | PointSize | Float | 0xc | 14.0 | 0xc | - | - | 0x9213e8d0 | - |
| 4 | Font | String | 0x0 | null | 0x10 | - | - | 0x9213e0d0 | - |
| 5 | TextStyle | Unsigned | 0x8 | 16 (0x10) | 0x18 | - | - | 0x9213e130 | - |
| 6 | LineSpacingAdjust | Integer | 0x0 | 0 | 0x1c | - | - | - | - |

### XuiButton

Base `XuiControl`. Registered at 0x92148188, getter 0x9213c848, table 0x92809f94.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | PressKey | Unsigned | 0x0 | 22592 (0x5840) | 0x4 | - | - | - | - |

### XuiNavButton

Base `XuiButton`. Registered at 0x92148248, getter 0x92145d40, table 0x9279a538.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | PressPath | String | 0x10 | null | 0x4 | - | - | - | - |
| 1 | StayVisible | Bool | 0x0 | false | 0x8 | 0x1 | - | - | - |
| 2 | SrcTransIndex | Unsigned | 0x0 | 0 | 0xc | - | - | - | - |
| 3 | DestTransIndex | Unsigned | 0x0 | 0 | 0x10 | - | - | - | - |

### XuiCheckbox

Base `XuiControl`. Registered at 0x921483b8, getter 0x9213c8d8, table 0x92809fc8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | PressKey | Unsigned | 0x0 | 22592 (0x5840) | 0x8 | - | - | - | - |

### XuiRadioButton

Base `XuiControl`. Registered at 0x92148478, getter 0x9213c968, table 0x92809ffc.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | PressKey | Unsigned | 0x0 | 22592 (0x5840) | 0x8 | - | - | - | - |

### XuiTextPresenter

Base `XuiElement`. Registered at 0x921485e8, getter 0x92145ec0, table 0x9280a808.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | TextColor | Color | 0x0 | 0xff000000 | 0x4 | - | - | - | - |
| 1 | DropShadowColor | Color | 0x0 | 0x80000000 | 0x8 | - | - | - | - |
| 2 | PointSize | Float | 0xc | 14.0 | 0xc | - | - | 0x9213e8d0 | - |
| 3 | Font | String | 0x0 | null | 0x10 | - | - | 0x9213e0d0 | - |
| 4 | TextStyle | Unsigned | 0x8 | 16 (0x10) | 0x18 | - | - | 0x9213e130 | - |
| 5 | LineSpacingAdjust | Integer | 0x0 | 0 | 0x1c | - | - | - | - |
| 6 | DataAssociation | Unsigned | 0x0 | 0 | 0x28 | - | - | - | - |

### XuiSound

Base `XuiElement`. Registered at 0x92148758, getter 0x921461a8, table 0x9280a960.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | State | Unsigned | 0x0 | 2 | 0x4 | - | - | 0x9216f940 | - |
| 1 | Loop | Bool | 0x8 | false | 0x8 | 0x4 | - | - | - |
| 2 | Finish | Bool | 0x8 | false | 0x8 | 0x8 | - | - | - |
| 3 | Volume | Float | 0x0 | -12.0 | 0xc | - | - | 0x92146148 | - |

### XuiListItem

Base `XuiCheckbox`. Registered at 0x92148828, getter 0x92146338, table 0x9280aa28.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Layout | Unsigned | 0x0 | 0 | 0x4 | - | - | 0x9213ea40 | - |
| 1 | Checkable | Bool | 0x0 | false | 0x8 | 0x2 | - | - | - |
| 2 | SelectedSize | Vector | 0x0 | (0.0, 0.0, 0.0) | 0xc | - | - | - | - |
| 3 | KeepSizeUnfocused | Bool | 0x0 | false | 0x8 | 0x4 | - | - | - |
| 4 | InterItemSpacing | Vector | 0x0 | (0.0, 0.0, 0.0) | 0x18 | - | - | - | - |

### XuiScrollEnd

Base `XuiControl`. Registered at 0x921488e8, getter 0x921431b8, table 0x9280a514.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Direction | Unsigned | 0x0 | 0 | 0x4 | - | - | 0x9213eaa8 | - |

### XuiScrollBar

Base `XuiControl`. Registered at 0x921489a8, getter 0x92143328, table 0x9280a578.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Direction | Unsigned | 0x0 | 0 | 0x4 | - | - | 0x9213eb10 | - |
| 1 | MinThumbSize | Unsigned | 0x0 | 10 (0xa) | 0x24 | - | - | - | - |

### XuiList

Base `XuiControl`. Registered at 0x92148a68, getter 0x92146510, table 0x9280ab1c.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Wrap | Bool | 0x0 | false | 0x98 | 0x1 | - | - | - |

### XuiProgressBar

Base `XuiControl`. Registered at 0x92148b38, getter 0x921466e8, table 0x9279a688.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | RangeMin | Integer | 0x8 | 0 | 0x8 | - | - | 0x9213ed90 | - |
| 1 | RangeMax | Integer | 0x8 | 100 | 0xc | - | - | 0x9213ede8 | - |
| 2 | Value | Integer | 0x0 | 0 | 0x10 | - | - | 0x9213ee40 | - |

### XuiSlider

Base `XuiControl`. Registered at 0x92148bf8, getter 0x92146830, table 0x9279a718.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | RangeMin | Integer | 0x8 | 0 | 0x10 | - | - | 0x9213ee98 | - |
| 1 | RangeMax | Integer | 0x8 | 100 | 0x14 | - | - | 0x9213eef0 | - |
| 2 | Value | Integer | 0x0 | 0 | 0x18 | - | - | 0x9213ef48 | - |
| 3 | Step | Integer | 0x0 | 1 | 0x1c | - | - | 0x9213efa0 | - |
| 4 | Vertical | Bool | 0x0 | false | 0x48 | - | 0x9213f068 | 0x9213eff8 | - |
| 5 | AccelInc | Integer | 0x0 | 0 | 0x20 | - | - | - | - |
| 6 | AccelTime | Unsigned | 0x0 | 0 | 0x24 | - | - | - | - |

### XuiEdit

Base `XuiControl`. Registered at 0x92148cb8, getter 0x92146ad0, table 0x9280ab60.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | TextLimit | Unsigned | 0x0 | 65534 (0xfffe) | 0x90 | - | - | 0x9213f228 | - |
| 1 | AllowedChars | String | 0x4 | null | 0x64 | - | - | 0x9213f280 | - |
| 2 | PasswordChar | String | 0x0 | null | 0x68 | - | - | 0x9213f2d8 | - |
| 3 | ReadOnly | Bool | 0x0 | false | 0x94 | 0x1 | - | 0x9213f330 | - |
| 4 | Multiline | Bool | 0x0 | false | 0x94 | 0x2 | - | 0x9213f388 | - |
| 5 | SmoothScroll | Bool | 0x0 | false | 0x94 | 0x4 | - | 0x9213f3e0 | - |

### XuiImagePresenter

Base `XuiElement`. Registered at 0x92148ee8, getter 0x9213f4f8, table 0x9280a030.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | SizeMode | Unsigned | 0x0 | 0 | 0x8 | - | - | 0x9213f438 | - |
| 1 | DataAssociation | Unsigned | 0x0 | 0 | 0x90 | - | - | - | - |
| 2 | BrushFlags | Unsigned | 0x0 | 0 | 0x84 | - | - | 0x9213f4a0 | - |

### XuiImage

Base `XuiElement`. Registered at 0x92148fa8, getter 0x92146d20, table 0x9280ac88.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | SizeMode | Unsigned | 0x0 | 0 | 0x8 | - | - | 0x9213f438 | - |
| 1 | ImagePath | String | 0x10 | null | 0x4 | - | - | 0x921436a8 | - |
| 2 | BrushFlags | Unsigned | 0x0 | 0 | 0x84 | - | - | 0x9213f4a0 | - |

### XuiTabScene

Base `XuiScene`. Registered at 0x92149068, getter 0x92146e68, table 0x9280ad20.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | TabCount | Unsigned | 0x0 | 1 | 0x8 | - | - | 0x9213f700 | - |
| 1 | Wrap | Bool | 0x0 | false | 0x4 | 0x1 | - | - | - |
| 2 | UserInterrupt | Bool | 0x0 | false | 0x4 | 0x2 | - | - | - |
| 3 | VerticalTabs | Bool | 0x0 | false | 0x4 | 0x10 | - | - | - |
| 4 | NoAutoHide | Bool | 0x0 | false | 0x4 | 0x20 | - | - | - |
| 5 | DefaultTab | Unsigned | 0x0 | 1 | 0x14 | - | - | 0x9213f688 | - |

### XuiNineGrid

Base `XuiElement`. Registered at 0x92149128, getter 0x92147098, table 0x9279a868.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | TextureFileName | String | 0x8 | null | 0x8 | - | - | 0x9213f770 | - |
| 1 | LeftOffset | Unsigned | 0x8 | 0 | 0xc | - | - | 0x9213f850 | - |
| 2 | TopOffset | Unsigned | 0x8 | 0 | 0x10 | - | - | 0x9213f858 | - |
| 3 | RightOffset | Unsigned | 0x8 | 0 | 0x14 | - | - | 0x9213f860 | - |
| 4 | BottomOffset | Unsigned | 0x8 | 0 | 0x18 | - | - | 0x9213f868 | - |
| 5 | NoCenter | Bool | 0x8 | false | 0x20 | - | - | 0x9213f870 | - |

### XuiTextureEffect

Base `XuiEffect`. Registered at 0x921491f8, getter 0x92143a70, table 0x9279a0b8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | TextureFileName | String | 0x8 | null | 0xf8 | - | - | 0x9213f9d0 | - |
| 1 | Translation | Vector | 0x0 | (0.0, 0.0, 0.0) | 0xb8 | - | - | 0x9213fa50 | - |
| 2 | Scale | Vector | 0x0 | (1.0, 1.0, 1.0) | 0xc8 | - | - | 0x9213fac8 | - |
| 3 | Rotation | Float | 0x0 | 0.0 | 0xc4 | - | - | 0x9213fb40 | - |
| 4 | ColorFactor | Quaternion | 0x0 | (1.0, 1.0, 1.0, 1.0) | 0xd4 | - | - | 0x9213fba8 | - |
| 5 | SrcBlendFactor | Unsigned | 0x8 | 4 | 0xe8 | - | - | 0x9213fc28 | - |
| 6 | DstBlendFactor | Unsigned | 0x8 | 5 | 0xec | - | - | 0x9213fc90 | - |
| 7 | WrapX | Unsigned | 0x0 | 2 | 0xf0 | - | 0x9213fcf8 | 0x9213fd58 | - |
| 8 | WrapY | Unsigned | 0x0 | 2 | 0xf0 | - | 0x9213fdd0 | 0x9213fe30 | - |
| 9 | BrushFlags | Unsigned | 0x0 | 0 | 0xf0 | - | 0x9213feb0 | 0x9213ff10 | - |

### XuiGrayscaleEffect

Base `XuiEffect`. Registered at 0x921492b8, getter 0x9213ffe0, table 0x9280a0c4.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | GrayAmount | Float | 0x0 | 1.0 | 0xa8 | - | - | 0x921404d0 | - |

### XuiRecolorEffect

Base `XuiEffect`. Registered at 0x92149378, getter 0x92143eb8, table 0x9280a718.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | ColorFactor | Quaternion | 0x0 | (1.0, 1.0, 1.0, 1.0) | 0xa8 | - | - | 0x921400f0 | - |

### XuiBrightPassEffect

Base `XuiEffect`. Registered at 0x92149438, getter 0x92143fd8, table 0x9280a764.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Threshold | Float | 0x0 | 0.75 | 0xa8 | - | - | 0x921404d0 | - |

### XuiHBlurEffect

Base `XuiEffect`. Registered at 0x921495b8, getter 0x92140380, table 0x9280a160.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Brightness | Float | 0x0 | 1.0 | 0xa8 | - | - | 0x921404d0 | - |
| 1 | BlurAmount | Float | 0x0 | 0.0 | 0xac | - | - | 0x92140538 | - |

### XuiVBlurEffect

Base `XuiEffect`. Registered at 0x92149678, getter 0x921405a0, table 0x9280a1c8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Brightness | Float | 0x0 | 1.0 | 0xa8 | - | - | 0x921404d0 | - |
| 1 | BlurAmount | Float | 0x0 | 0.0 | 0xac | - | - | 0x92140538 | - |

### XuiElement

No base class. Registered at 0x9214ab08, getter 0x92149960, table 0x9280ae80.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Id | String | 0x0 | null | 0x4 | - | - | - | - |
| 1 | Width | Float | 0x0 | 60.0 | 0xc | - | - | 0x92145450 | - |
| 2 | Height | Float | 0x0 | 30.0 | 0x10 | - | - | 0x921454f8 | - |
| 3 | Position | Vector | 0x0 | (0.0, 0.0, 0.0) | 0x30 | - | - | 0x9213bf08 | - |
| 4 | Scale | Vector | 0x0 | (1.0, 1.0, 1.0) | 0x3c | - | - | 0x9213bf18 | - |
| 5 | Rotation | Quaternion | 0x0 | (0.0, 0.0, 0.0, 1.0) | 0x48 | - | - | 0x9213bf78 | - |
| 6 | Opacity | Float | 0x0 | 1.0 | 0x14 | - | - | - | - |
| 7 | Anchor | Unsigned | 0x0 | 0 | 0xe8 | - | - | 0x9213c018 | - |
| 8 | Pivot | Vector | 0x0 | (0.0, 0.0, 0.0) | 0x58 | - | - | 0x9213bf88 | - |
| 9 | Show | Bool | 0x0 | true | 0xf0 | 0x1 | - | 0x92145698 | - |
| 10 | BlendMode | Unsigned | 0x0 | 0 | 0xec | - | - | 0x9213df60 | - |
| 11 | DisableTimelineRecursion | Bool | 0x0 | false | 0xf0 | 0x80 | - | - | - |
| 12 | DesignTime | Bool | 0x8 | false | 0xf0 | 0x40 | - | - | - |
| 13 | ColorWriteFlags | Unsigned | 0x0 | 0 | 0xf0 | - | 0x9213dff0 | 0x9213dfa8 | - |
| 14 | ClipChildren | Bool | 0x0 | false | 0xf0 | 0x1000 | - | - | - |
| 15 | EnableEffects | Bool | 0x0 | false | 0xf0 | 0x4000 | - | 0x9213bf98 | - |
| 16 | DisableFocusRecursion | Bool | 0x0 | false | 0xf0 | 0x10000 | - | 0x921455a0 | - |

### XuiControl

Base `XuiElement`. Registered at 0x9214aba8, getter 0x92149f58, table 0x9279a988.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | ClassOverride | String | 0x8 | null | 0x4 | - | - | - | - |
| 1 | Visual | String | 0x0 | null | 0x8 | - | - | 0x9215b458 | - |
| 2 | Enabled | Bool | 0x0 | true | 0x10 | 0x1 | - | 0x9215b508 | - |
| 3 | UnfocusedInput | Bool | 0x0 | false | 0x10 | 0x2 | - | - | - |
| 4 | NavLeft | String | 0x0 | null | 0x14 | - | - | - | - |
| 5 | NavRight | String | 0x0 | null | 0x18 | - | - | - | - |
| 6 | NavUp | String | 0x0 | null | 0x1c | - | - | - | - |
| 7 | NavDown | String | 0x0 | null | 0x20 | - | - | - | - |
| 8 | NavTabForward | String | 0x0 | null | 0x24 | - | - | - | - |
| 9 | NavTabBackward | String | 0x0 | null | 0x28 | - | - | - | - |
| 10 | Text | String | 0x4 | null | 0x2c | - | 0x92145a88 | 0x92145a60 | - |
| 11 | PointSize | Float | 0x6 | -1.0 | 0x30 | - | - | - | - |
| 12 | ImagePath | String | 0x10 | null | 0x34 | - | - | - | - |

### XuiEffect

Base `XuiElement`. Registered at 0x9214acf8, getter 0x921472e8, table 0x9280ae48.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Id | String | 0x0 | null | 0x4 | - | - | 0x9213f8e0 | - |

### XuiHVBlurEffect

Base `XuiEffect`. Registered at 0x9214ada8, getter 0x92140230, table 0x9280a0f8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Brightness | Float | 0x0 | 1.0 | 0xa8 | - | - | 0x921404d0 | - |
| 1 | BlurAmount | Float | 0x0 | 0.0 | 0xac | - | - | 0x92140538 | - |

### XuiFigure

Base `XuiElement`. Registered at 0x9214af10, getter 0x92142d20, table 0x92799ff8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Stroke | Compound | 0x2 | - | 0xa0 | - | - | - | - |
| 0 | Stroke.StrokeWidth | Float | 0x0 | 0.0 | 0x4 | - | - | 0x9213c590 | - |
| 1 | Stroke.StrokeColor | Color | 0x0 | 0xff0f0feb | 0x8 | - | - | 0x9213c5d0 | - |
| 1 | Fill | Compound | 0x0 | - | 0x14 | - | - | - | - |
| 0 | Fill.FillType | Unsigned | 0x2 | 1 | 0x88 | - | - | 0x9213e590 | - |
| 1 | Fill.FillColor | Color | 0x2 | 0xff0f0f80 | 0xc | - | - | 0x9213c638 | - |
| 2 | Fill.TextureFileName | String | 0x2 | null | 0x10 | - | - | 0x9213e4b0 | - |
| 3 | Fill.Gradient | Compound | 0x2 | - | 0x14 | - | - | - | - |
| 0 | Fill.Gradient.Radial | Bool | 0x0 | false | 0xc | 0x1 | - | 0x9213e190 | - |
| 1 | Fill.Gradient.NumStops | Integer | 0xa | 0 | 0x4 | - | - | 0x92142298 | - |
| 2 | Fill.Gradient.StopColor | Color | 0x1 | - | 0x8 | - | 0x9213e2d8 | 0x9213e1f0 | 0x9213c610 |
| 3 | Fill.Gradient.StopPos | Float | 0x1 | - | 0x8 | - | 0x9213e340 | 0x9213e260 | 0x9213c610 |
| 4 | Fill.Translation | Vector | 0x0 | (0.0, 0.0, 0.0) | 0x28 | - | - | 0x9213e4f0 | - |
| 5 | Fill.Scale | Vector | 0x0 | (1.0, 1.0, 1.0) | 0x34 | - | - | 0x9213e540 | - |
| 6 | Fill.Rotation | Float | 0x0 | 0.0 | 0x40 | - | - | 0x9213c678 | - |
| 7 | Fill.WrapX | Unsigned | 0x0 | 0 | 0x84 | - | 0x9213e618 | 0x9213c6b8 | - |
| 8 | Fill.WrapY | Unsigned | 0x0 | 0 | 0x84 | - | 0x9213e668 | 0x9213c700 | - |
| 9 | Fill.BrushFlags | Unsigned | 0x0 | 0 | 0x84 | - | 0x9213e6b8 | 0x9213c748 | - |
| 10 | Fill.TransformVersion | Unsigned | 0xa | 0 | 0x84 | - | 0x9213e708 | 0x9213c788 | - |
| 2 | Closed | Bool | 0x2 | - | 0xc8 | 0x2 | - | - | - |
| 3 | Points | Custom | 0x2 | - | 0x4 | - | - | - | - |

### XuiScene

Base `XuiControl`. Registered at 0x9214b118, getter 0x92145ac8, table 0x9279a3e8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | DefaultFocus | String | 0x8 | null | 0x4 | - | - | - | - |
| 1 | TransFrom | String | 0x0 | null | 0x28 | - | - | - | - |
| 2 | TransTo | String | 0x0 | null | 0x2c | - | - | - | - |
| 3 | TransBackFrom | String | 0x0 | null | 0x30 | - | - | - | - |
| 4 | TransBackTo | String | 0x0 | null | 0x34 | - | - | - | - |
| 5 | InterruptTransitions | Unsigned | 0x8 | 0 | 0x3c | - | - | 0x9213e758 | - |
| 6 | IgnorePresses | Bool | 0x0 | false | 0x38 | 0x10 | - | - | - |

### XuiCommonList

Base `XuiList`. Registered at 0x9214b2b8, getter 0x921465a0, table 0x9279a5f8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | ItemsText | String | 0x4 | null | 0x4 | - | - | 0x9213ec28 | - |
| 1 | ItemsImage | String | 0x0 | null | 0x8 | - | - | 0x9213eca0 | - |
| 2 | ItemsNavPath | String | 0x0 | null | 0xc | - | - | 0x9213ed18 | - |

### XuiSoundXAudio

Base `XuiSound`. Registered at 0x9214c5b8, getter 0x9214ba90, table 0x9279abf8.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | File | String | 0x10 | null | 0x4 | - | - | 0x9214b7e0 | - |

### XuiHtmlElement

Base `XuiElement`. Registered at 0x92197a48, getter 0x921950c8, table 0x9279aeec.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Text | String | 0x4 | null | 0x6c | - | - | 0x9219d338 | - |

### XuiHtmlPresenter

Base `XuiElement`. Registered at 0x92198f30, getter 0x921951c8, table 0x9280b988.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | DataAssociation | Unsigned | 0x0 | 0 | 0x70 | - | - | - | - |

### XuiHtmlControl

Base `XuiControl`. Registered at 0x92199970, getter 0x92199058, table 0x9279af1c.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | LineHeight | Integer | 0xc | 28 | 0x54 | - | - | - | - |

### DashVideo

Base `XuiElement`. Registered at 0x921b01c0, getter 0x921af9f8, table 0x9279b180.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | File | String | 0x10 | null | 0x28 | - | - | 0x921af868 | - |
| 1 | SizeMode | Unsigned | 0x0 | 0 | 0x8 | - | - | 0x921af8d8 | - |

### XuiBOTDOfflineScene

Base `XuiScene`. Registered at 0x922577a8, getter 0x92256f00, table 0x92874598.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | ButtonVisual | String | 0x0 | null | 0x8 | - | 0x92257b18 | 0x92257948 | - |

### XuiBOTDOfflineContainer

Base `XuiScene`. Registered at 0x92257840, getter 0x922576a8, table 0x92874600.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Area | Unsigned | 0x0 | 0 | 0x214 | - | 0x922572b8 | 0x92257080 | - |
| 1 | DefaultBanner | String | 0x0 | null | 0x8 | - | 0x92257268 | 0x92257008 | - |

### MediaTwist

Base `XuiControl`. Registered at 0x92272ce0, getter 0x92272c30, table 0x927c1b20.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Active | Integer | 0x8 | 0 | 0x10c | - | - | 0x92272b00 | - |

### XuiGamerCard

Base `XuiControl`. Registered at 0x9227fef0, getter 0x9227f288, table 0x92887e90.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Format | String | 0x0 | "Normal" | 0xc | - | 0x9227efa0 | 0x9227f000 | - |
| 1 | ShowExtendedPanel | Bool | 0x0 | true | 0xc | - | 0x9227f080 | 0x9227f0d8 | - |

### ScriptData

Base `XuiElement`. Registered at 0x9258c7d8, getter 0x9258ba18, table 0x927fc990.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Query | String | 0x8 | null | 0x14 | - | 0x925894b0 | 0x925894a0 | - |
| 1 | State | Integer | 0x8 | 0 | 0x8 | - | - | - | - |
| 2 | ItemCount | Unsigned | 0x8 | 0 | 0xc | - | 0x92589540 | 0x92378080 | - |
| 3 | Selected | Integer | 0x8 | 0 (uint value) | 0x10 | - | 0x92589500 | 0x925894f0 | - |

### ScriptScene

Base `XuiScene`. Registered at 0x9258c870, getter 0x92588658, table 0x927fc930.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Script | String | 0x18 | null | 0xc | - | - | - | - |
| 1 | Visible | Bool | 0x8 | true | 0x8 | - | - | - | - |

### ScriptButton

Base `XuiButton`. Registered at 0x9258caa8, getter 0x9258c138, table 0x927fca50.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | DataSet | String | 0x8 | null | 0x14 | - | - | 0x9258b8c0 | - |
| 1 | DataAssociation | String | 0x8 | null | 0x18 | - | - | 0x92589410 | - |
| 2 | Embedded | String | 0x8 | null | 0x44 | - | - | - | - |
| 3 | Item | Integer | 0x8 | -1 | 0x4c | - | - | - | - |

### ScriptList

Base `XuiList`. Registered at 0x9258cb40, getter 0x9258c2c8, table 0x927fcb10.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | DataSet | String | 0x8 | null | 0x14 | - | - | 0x9258b8c0 | - |
| 1 | DataAssociation | String | 0x8 | null | 0x18 | - | - | 0x92589410 | - |
| 2 | Embedded | String | 0x8 | null | 0x44 | - | - | - | - |

### ScriptImage

Base `XuiControl`. Registered at 0x9258cbd8, getter 0x9258c580, table 0x927fcba0.

| Bit | Name | Type | Flags | Default | Offset | Mask | Get | Set | Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | DataSet | String | 0x8 | null | 0x14 | - | - | 0x9258b8c0 | - |
| 1 | DataAssociation | String | 0x8 | null | 0x18 | - | - | 0x92589410 | - |
| 2 | Embedded | String | 0x8 | null | 0x44 | - | - | - | - |
| 3 | Item | Integer | 0x8 | -1 | 0x4c | - | - | - | - |

## Reimplementation

`exui-core::schema` holds the tables as statics. A compound property's sub-definitions live in its `PropType::Compound`. Definitions are identified by address, since distinct classes declare equal-looking records. `Registry::new` registers the `XuiInit` built-ins; `schema::xui::register_sound_xaudio`, `schema::xui::register_html` and `schema::dash::register` add the other groups. `Registry::register` enforces the duplicate and base-first rules, and `ClassRef::levels` walks a class root first without allocating.
