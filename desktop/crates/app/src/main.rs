#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use gpui_kit::{
    assets::Assets,
    component::{Root, TitleBar},
    *,
};
use kernelledger_ui::Workspace;
use std::borrow::Cow;

gpui_kit::assets::icon_assets!(
    AppIcons,
    [
        Activity,
        ArrowUp,
        BookOpen,
        BookmarkPlus,
        ChevronDown,
        ChevronLeft,
        ChevronRight,
        ChevronUp,
        ChevronsDown,
        ChevronsUp,
        CircleAlert,
        Clock,
        Code,
        Copy,
        Cpu,
        Database,
        Eye,
        EyeOff,
        Ellipsis,
        FileText,
        Flag,
        FolderOpen,
        Info,
        Moon,
        NotebookPen,
        OctagonX,
        PanelLeft,
        Play,
        Plus,
        Power,
        RefreshCw,
        RotateCcw,
        Search,
        Settings,
        Sparkles,
        Square,
        Stethoscope,
        Sun,
        Terminal,
        Trash,
        TriangleAlert,
        Type,
        X,
    ]
);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match AppIcons.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(AppIcons.list(path)?);
        Ok(paths)
    }
}

fn main() {
    let project = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    // Read small profile preferences before entering the UI event loop.
    let profile = kernelledger_client::profile_path();
    let appearance =
        std::fs::read_to_string(profile.join("appearance.txt")).unwrap_or_else(|_| "system".into());
    let recent = kernelledger_client::recent::load(&profile);
    // WebView2 needs Direct Composition off. WebKitGTK embeds only into X11.
    #[cfg(target_os = "windows")]
    unsafe {
        std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "true");
    }
    #[cfg(target_os = "linux")]
    let app = gpui_kit::platform::linux(gpui_kit::platform::WindowingModes::X11);
    #[cfg(not(target_os = "linux"))]
    let app = gpui_kit::application();
    app.with_assets(AppAssets).run(move |cx| {
        gpui_kit::component::init(cx);
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1280.), px(850.)), cx);
        let mut chrome = TitleBar::window_options();
        // Centers the macOS window controls on the 44px workbench title bar.
        if let Some(titlebar) = chrome.titlebar.as_mut() {
            titlebar.traffic_light_position = Some(point(px(14.), px(15.)));
        }
        cx.open_window(
            WindowOptions {
                window_min_size: Some(size(px(800.), px(600.))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..chrome
            },
            |window, cx| {
                let view = cx.new(|cx| Workspace::new(project, &appearance, recent, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("Unable to open a graphics window; see docs/desktop/platforms.md");
    });
}
