use std::convert::TryInto;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};

use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};

// ============================================================
// Application state
// ============================================================

struct AppState {
    registry_state: RegistryState,

    compositor: CompositorState,

    output_state: OutputState,

    shm: Shm,

    layer: LayerSurface,

    pool: SlotPool,

    width: u32,
    height: u32,

    configured: bool,
}

// ============================================================
// CompositorHandler
// ============================================================

impl CompositorHandler for AppState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Nothing to do.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Nothing to do.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        // No animation.
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Nothing to do.
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Nothing to do.
    }
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

// ============================================================
// LayerShellHandler
// ============================================================

impl LayerShellHandler for AppState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        println!("Layer surface closed.");
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        /*
         * The compositor tells us the actual size.
         *
         * If it sends 0, keep our requested size.
         */

        if configure.new_size.0 != 0 {
            self.width = configure.new_size.0;
        }

        if configure.new_size.1 != 0 {
            self.height = configure.new_size.1;
        }

        println!("Configured: {}x{}", self.width, self.height);

        /*
         * Only draw after the first configure.
         */

        if !self.configured {
            self.configured = true;

            self.draw(qh);
        }
    }
}

// ============================================================
// SHM
// ============================================================

impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

// ============================================================
// Drawing
// ============================================================

impl AppState {
    fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let width = self.width;
        let height = self.height;

        let stride = (width * 4) as i32;

        // ----------------------------------------------------
        // Create a shared-memory buffer
        // ----------------------------------------------------

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("Failed to create buffer");

        // ----------------------------------------------------
        // Fill the entire buffer with one color.
        //
        // ARGB:
        //
        // FF = alpha
        // 30 = red
        // 30 = green
        // 30 = blue
        //
        // Result:
        //
        //       dark gray rectangle
        // ----------------------------------------------------

        let color = 0xFF303030u32;

        for pixel in canvas.chunks_exact_mut(4) {
            let pixel_array: &mut [u8; 4] = pixel.try_into().unwrap();

            *pixel_array = color.to_le_bytes();
        }

        // ----------------------------------------------------
        // Tell Wayland which part changed
        // ----------------------------------------------------

        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);

        // ----------------------------------------------------
        // Attach buffer
        // ----------------------------------------------------

        buffer
            .attach_to(self.layer.wl_surface())
            .expect("Failed to attach buffer");

        // ----------------------------------------------------
        // Commit
        // ----------------------------------------------------

        self.layer.commit();

        println!("Displayed {}x{} rectangle.", width, height);
    }
}

// ============================================================
// Registry
// ============================================================
//
// IMPORTANT:
//
// This is separate from delegate_dispatch2!
// ============================================================

delegate_registry!(AppState);

impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    /*
     * We don't need dynamic outputs/seats/etc.
     */

    registry_handlers![OutputState,];
}

// ============================================================
// SCTK 0.21.1 dispatch system
// ============================================================

smithay_client_toolkit::delegate_dispatch2!(AppState);

// ============================================================
// Main
// ============================================================

fn main() {
    // --------------------------------------------------------
    // 1. Connect to Wayland
    // --------------------------------------------------------

    let connection = Connection::connect_to_env().expect("Failed to connect to Wayland");

    println!("Connected to Wayland.");

    // --------------------------------------------------------
    // 2. Get globals
    // --------------------------------------------------------

    let (globals, mut event_queue) =
        registry_queue_init(&connection).expect("Failed to initialize Wayland registry");

    let qh = event_queue.handle();

    // --------------------------------------------------------
    // 3. Get wl_compositor
    // --------------------------------------------------------

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");

    println!("Got wl_compositor.");

    // --------------------------------------------------------
    // 4. Get wl_shm
    // --------------------------------------------------------

    let shm = Shm::bind(&globals, &qh).expect("wl_shm is not available");

    println!("Got wl_shm.");

    // --------------------------------------------------------
    // 5. Get wlr-layer-shell
    // --------------------------------------------------------

    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell is not available");

    println!("Got wlr-layer-shell.");

    // --------------------------------------------------------
    // 6. Create wl_surface
    // --------------------------------------------------------

    let surface = compositor.create_surface(&qh);

    println!("Created wl_surface.");

    // --------------------------------------------------------
    // 7. Turn wl_surface into layer surface
    // --------------------------------------------------------

    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("simple-rectangle"),
        None,
    );

    println!("Created layer surface.");

    // --------------------------------------------------------
    // 8. Configure layer
    // --------------------------------------------------------

    layer.set_size(500, 300);

    /*
     * No anchors:
     *
     *     centered
     *
     * roughly:
     *
     *       ┌─────────────┐
     *       │             │
     *       │  500 x 300  │
     *       │             │
     *       └─────────────┘
     *
     */

    layer.set_anchor(Anchor::empty());

    /*
     * Don't reserve screen space.
     */

    layer.set_exclusive_zone(0);

    // --------------------------------------------------------
    // 9. Initial commit
    // --------------------------------------------------------
    //
    // No buffer yet.
    //
    // This asks the compositor to send us configure().
    // --------------------------------------------------------

    layer.commit();

    // --------------------------------------------------------
    // 10. Create SHM pool
    // --------------------------------------------------------

    let pool = SlotPool::new(500 * 300 * 4, &shm).expect("Failed to create SHM pool");

    // --------------------------------------------------------
    // 11. Create state
    // --------------------------------------------------------

    let output_state = OutputState::new(&globals, &qh);

    let mut state = AppState {
        registry_state: RegistryState::new(&globals),

        compositor,

        output_state,

        shm,

        layer,

        pool,

        width: 500,
        height: 300,

        configured: false,
    };

    // --------------------------------------------------------
    // 12. Event loop
    // --------------------------------------------------------

    loop {
        event_queue
            .blocking_dispatch(&mut state)
            .expect("Wayland event dispatch failed");
    }
}
