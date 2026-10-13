use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    frame::{Frame, ImageFormat},
    graphics_capture_api::InternalCaptureControl,
    settings::*,
    window::Window,
};

struct Capture(String);
impl GraphicsCaptureApiHandler for Capture {
    type Flags = String;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    fn new(context: Context<String>) -> Result<Self, Self::Error> {
        Ok(Self(context.flags))
    }
    fn on_frame_arrived(&mut self, frame: &mut Frame, control: InternalCaptureControl) -> Result<(), Self::Error> {
        frame.save_as_image(&self.0, ImageFormat::Png)?;
        control.stop();
        Ok(())
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let hwnd: usize = args[1].parse().unwrap();
    Capture::start(Settings::new(
        Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void),
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Rgba8,
        args[2].clone(),
    )).expect("Capture the requested probe window");
}
