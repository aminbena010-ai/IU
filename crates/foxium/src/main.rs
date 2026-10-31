use eframe::egui;
use std::path::PathBuf;

struct FoxiumApp {
    url: String,
    status: String,
    image_path: PathBuf,
    texture: Option<egui::TextureHandle>,
    loading: bool,
}

impl Default for FoxiumApp {
    fn default() -> Self {
        Self {
            url: "https://www.google.com".to_string(),
            status: "Listo.".to_string(),
            image_path: PathBuf::from("iu_render.png"),
            texture: None,
            loading: false,
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

    fn navigate(&mut self, ctx: &egui::Context) {
        self.loading = true;
        self.status = format!("Cargando {} ...", self.url);
        let url = self.url.clone();
        browser::start(Some(url));
        self.load_image(ctx);
        self.loading = false;
    }
}

impl eframe::App for FoxiumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("barra").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Foxium");
                ui.separator();
                ui.label("URL:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .desired_width(f32::INFINITY)
                        .hint_text("https://..."),
                );
                if ui.button("Ir").clicked() && !self.loading {
                    self.navigate(ctx);
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
