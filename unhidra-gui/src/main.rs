use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("UNHIDRA RETRO GUI")
        .with_inner_size(winit::dpi::LogicalSize::new(480.0, 320.0))
        .build(&event_loop)
        .unwrap();

    // Temporary blank window. Rendering comes next.
    println!("UNHIDRA RETRO GUI started");

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    elwt.exit();
                }

                _ => {}
            },

            _ => {}
        }
    }).unwrap();
}
