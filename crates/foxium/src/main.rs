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
        if !self.image_path.exists() {
            self.status = format!("No existe {}", self.image_path.display());
            return;
        }
        match image::open(&self.image_path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let pixels = rgba.into_raw();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                self.texture = Some(ctx.load_texture(
                    "iu_render",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
                self.status = "Render cargado.".to_string();
            }
            Err(e) => {
                self.status = format!("Error cargando imagen: {}", e);
            }
        }
    }

    fn navigate(&mut self) {
        self.loading = true;
        self.status = format!("Cargando {} ...", self.url);
        
        let url = self.url.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);

        // Ejecutamos el motor en un hilo separado para no congelar la UI
        thread::spawn(move || {
            browser::start(Some(url));
            let _ = tx.send(());
        });
    }

    fn check_thread(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.rx {
            if let Ok(_) = rx.try_recv() {
                self.load_image(ctx);
                self.loading = false;
                self.rx = None;
            } else {
                // Mantener la UI viva mientras carga
                ctx.request_repaint();
            }
        }
    }
}

impl eframe::App for FoxiumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_thread(ctx);

        egui::TopBottomPanel::top("barra").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Foxium");
                ui.separator();
                ui.label("URL:");
                
                // Forzamos un ancho máximo para que el botón no se salga
                ui.add_sized(
                    [ui.available_width() - 80.0, 24.0],
                    egui::TextEdit::singleline(&mut self.url).hint_text("https://..."),
                );
                
                // Botón "Ir" fijo
                if ui.add_enabled(!self.loading, egui::Button::new("Ir")).clicked() {
                    self.navigate();
                }
            });
        });

        egui::TopBottomPanel::bottom("estado").show(ctx, |ui| {
            ui.label(&self.status);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                if let Some(texture) = &self.texture {
                    let size = texture.size_vec2();
                    ui.image((texture.id(), size));
                } else {
                    ui.label("Sin render. Escribe una URL y pulsa 'Ir'.");
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
