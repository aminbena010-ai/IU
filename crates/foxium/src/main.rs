use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

struct FoxiumApp {
    url: String,
    status: String,
    image_path: PathBuf,
    texture: Option<egui::TextureHandle>,
    loading: bool,
    rx: Option<Receiver<()>>,
}

impl Default for FoxiumApp {
    fn default() -> Self {
        Self {
            url: "https://www.google.com".to_string(),
            status: "Listo.".to_string(),
            image_path: PathBuf::from("iu_render.png"),
            texture: None,
            loading: false,
            rx: None,
        }
    }
}

impl FoxiumApp {
    fn load_image(&mut self, ctx: &egui::Context) {
        if !self.image_path.exists() { return; }
        if let Ok(img) = image::open(&self.image_path) {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            self.texture = Some(ctx.load_texture("iu_render", color_image, egui::TextureOptions::LINEAR));
            self.status = "Render cargado.".to_string();
        }
    }

    fn navigate(&mut self) {
        self.loading = true;
        self.status = format!("Cargando {} ...", self.url);
        let url = self.url.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            browser::start(Some(url));
            let _ = tx.send(());
        });
    }

    fn check_thread(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.rx {
            if rx.try_recv().is_ok() {
                self.load_image(ctx);
                self.loading = false;
                self.rx = None;
            } else {
                ctx.request_repaint();
            }
        }
    }
}

impl eframe::App for FoxiumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_thread(ctx);

        // Ajustar el estilo visual para que no haya fondos oscuros ni márgenes
        let mut visuals = egui::Visuals::light();
        visuals.window_fill = egui::Color32::WHITE;
        visuals.panel_fill = egui::Color32::from_rgb(240, 240, 240);
        ctx.set_visuals(visuals);

        // Barra superior compacta
        egui::TopBottomPanel::top("barra").exact_height(36.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Foxium").strong());
                ui.separator();
                ui.add_sized(
                    [ui.available_width() - 60.0, 24.0],
                    egui::TextEdit::singleline(&mut self.url).hint_text("https://..."),
                );
                if ui.add_enabled(!self.loading, egui::Button::new("Ir")).clicked() {
                    self.navigate();
                }
            });
        });

        // Barra de estado minimalista
        egui::TopBottomPanel::bottom("estado").exact_height(24.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&self.status).size(12.0).weak());
            });
        });

        // Área central: el render del navegador ocupa todo el espacio
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::WHITE)).show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                if let Some(texture) = &self.texture {
                    let size = texture.size_vec2();
                    // Escalar la imagen para que llene el ancho disponible
                    let scale = (ui.available_width() / size.x).min(1.0);
                    let new_size = size * scale;
                    ui.image((texture.id(), new_size));
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Escribe una URL y pulsa 'Ir'.");
                    });
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 900.0])
            .with_title("Foxium"),
        ..Default::default()
    };
    eframe::run_native(
        "Foxium",
        options,
        Box::new(|_cc| Ok(Box::new(FoxiumApp::default()))),
    )
}
