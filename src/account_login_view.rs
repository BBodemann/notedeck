use crate::app_style::NotedeckTextStyle;
use crate::key_parsing::{perform_key_retrieval, LoginError};
use crate::login_manager::LoginManager;
use egui::{
    Align, Align2, Button, Color32, Frame, Id, LayerId, Margin, Pos2, Rect,
    RichText, Rounding, Ui, Vec2, Window,
};
use egui::{Image, TextBuffer, TextEdit};

pub struct AccountLoginView<'a> {
    ctx: &'a egui::Context,
    manager: &'a mut LoginManager,
    generate_y_intercept: Option<f32>,
}

impl<'a> AccountLoginView<'a> {
    pub fn new(ctx: &'a egui::Context, manager: &'a mut LoginManager) -> Self {
        AccountLoginView {
            ctx,
            manager,
            generate_y_intercept: None,
        }
    }

    pub fn panel(&mut self) {
        let frame = egui::CentralPanel::default();

        let screen_width = self.ctx.screen_rect().max.x;
        let screen_height = self.ctx.screen_rect().max.y;

        frame.show(self.ctx, |ui| {
            let title_layer = LayerId::new(egui::Order::Background, Id::new("Title layer"));

            let mut top_panel_height: Option<f32> = None;
            ui.with_layer_id(title_layer, |ui| {
                egui::TopBottomPanel::top("Top")
                    .resizable(false)
                    .default_height(340.0)
                    .frame(Frame::none())
                    .show_separator_line(false)
                    .show_inside(ui, |ui| {
                        top_panel_height = Some(ui.available_rect_before_wrap().bottom());
                        self.top_title_area(ui);
                    });
            });

            egui::TopBottomPanel::bottom("Bottom")
                .resizable(false)
                .frame(Frame::none())
                .show_separator_line(false)
                .show_inside(ui, |ui| {
                    self.window(ui, top_panel_height.unwrap_or(0.0));
                });

            let top_rect = Rect {
                min: Pos2::ZERO,
                max: Pos2::new(
                    screen_width,
                    self.generate_y_intercept.unwrap_or(screen_height * 0.5),
                ),
            };

            let top_background_color = ui.visuals().noninteractive().bg_fill;
            ui.painter_at(top_rect)
                .with_layer_id(LayerId::background())
                .rect_filled(top_rect, Rounding::ZERO, top_background_color);
        });
    }

    fn window(&mut self, ui: &mut Ui, top_panel_height: f32) {
        let needed_height_over_top = (self.ctx.screen_rect().bottom() / 2.0) - 230.0;
        let y_offset = if top_panel_height > needed_height_over_top {
            top_panel_height - needed_height_over_top
        } else {
            0.0
        };
        Window::new("Account login")
            .movable(false)
            .constrain(true)
            .collapsible(false)
            .drag_to_scroll(false)
            .title_bar(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0f32, y_offset])
            .max_width(538.0)
            .frame(egui::Frame::window(ui.style()).inner_margin(Margin::ZERO))
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);

                    ui.label(login_title_text());

                    ui.add_space(16f32);

                    ui.label(login_window_info_text(ui));

                    ui.add_space(24.0);

                    Frame::none()
                        .outer_margin(Margin::symmetric(48.0, 0.0))
                        .show(ui, |ui| {
                            self.login_form(ui);
                        });

                    ui.add_space(32.0);

                    let y_margin: f32 = 24.0;
                    let generate_frame = egui::Frame::default()
                        .fill(ui.style().noninteractive().bg_fill) // TODO: gradient
                        .rounding(ui.style().visuals.window_rounding)
                        .stroke(ui.style().noninteractive().bg_stroke)
                        .inner_margin(Margin::symmetric(48.0, y_margin));

                    generate_frame.show(ui, |ui| {
                        self.generate_y_intercept =
                            Some(ui.available_rect_before_wrap().top() - y_margin);
                        self.generate_group(ui);
                    });
                });
            });
    }

    fn top_title_area(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add(logo_unformatted().max_width(232.0));

            ui.add_space(48.0);

            let welcome_data = egui::include_image!("../assets/Welcome to Nostrdeck 2x.png");
            ui.add(egui::Image::new(welcome_data).max_width(528.0));

            ui.add_space(12.0);

            // ui.label(
            //     RichText::new("Welcome to Nostrdeck")
            //         .size(48.0)
            //         .strong()
            //         .line_height(Some(72.0)),
            // );
            ui.label(login_info_text());
        });
    }

    fn login_form(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered_justified(|ui| {
            ui.horizontal(|ui| {
                ui.label(login_textedit_info_text());
            });

            ui.add_space(8f32);

            ui.add(
                login_textedit(&mut self.manager.login_key).min_size(Vec2::new(440.0, 40.0)),
            );

            ui.add_space(8.0);

            ui.vertical_centered(|ui| {
                if self.manager.promise.is_some() {
                    ui.add(egui::Spinner::new());
                }
            });

            if let Some(error_key) = &self.manager.key_on_error {
                if self.manager.login_key != *error_key {
                    self.manager.error = None;
                    self.manager.key_on_error = None;
                }
            }
            if let Some(err) = &self.manager.error {
                ui.horizontal(|ui| {
                    let error_label = match err {
                        LoginError::InvalidKey => egui::Label::new(
                            RichText::new("Invalid key.").color(ui.visuals().error_fg_color),
                        ),
                        LoginError::Nip05Failed(e) => {
                            egui::Label::new(RichText::new(e).color(ui.visuals().error_fg_color))
                        }
                    };
                    ui.add(error_label.truncate(true));
                });
            }

            ui.add_space(8.0);

            let login_button = login_button().min_size(Vec2::new(442.0, 40.0));

            if ui.add(login_button).clicked() {
                self.manager.promise = Some(perform_key_retrieval(&self.manager.login_key));
            }
        });
    }

    fn generate_group(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("New in nostr?").text_style(NotedeckTextStyle::Heading3.text_style()));

            ui.label(
                RichText::new(" — we got you!")
                    .text_style(NotedeckTextStyle::Heading3.text_style())
                    .color(ui.visuals().noninteractive().fg_stroke.color),
            );
        });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(generate_info_text().color(ui.visuals().noninteractive().fg_stroke.color));
        });

        ui.add_space(16.0);

        let generate_button = generate_keys_button().min_size(Vec2::new(442.0, 40.0));
        if ui.add(generate_button).clicked() {
            // TODO: keygen
        }
    }
}

fn login_title_text() -> RichText {
    RichText::new("Login")
        .text_style(NotedeckTextStyle::Heading2.text_style())
        .strong()
}

fn login_info_text() -> RichText {
    RichText::new("The best alternative to tweetDeck built in nostr protocol")
        .text_style(NotedeckTextStyle::Heading3.text_style())
}

fn login_window_info_text(ui: &Ui) -> RichText {
    RichText::new("Enter your private key to start using Notedeck")
        .text_style(NotedeckTextStyle::Body.text_style())
        .color(ui.visuals().noninteractive().fg_stroke.color)
}

fn login_textedit_info_text() -> RichText {
    RichText::new("Enter your key")
        .strong()
        .text_style(NotedeckTextStyle::Body.text_style())
}

fn logo_unformatted() -> Image<'static> {
    let logo_gradient_data = egui::include_image!("../assets/Logo-Gradient-2x.png");
    return egui::Image::new(logo_gradient_data);
}

fn generate_info_text() -> RichText {
    RichText::new("Quickly generate your keys. Make sure you save them safely.")
        .text_style(NotedeckTextStyle::Body.text_style())
}

fn generate_keys_button() -> Button<'static> {
    Button::new(RichText::new("Generate keys").text_style(NotedeckTextStyle::Body.text_style()))
}

fn login_button() -> Button<'static> {
    Button::new(
        RichText::new("Login now — let's do this!")
            .text_style(NotedeckTextStyle::Body.text_style())
            .strong(),
    )
    .fill(Color32::from_rgb(0xF8, 0x69, 0xB6)) // TODO: gradient
    .min_size(Vec2::new(0.0, 40.0))
}

fn login_textedit(text: &mut dyn TextBuffer) -> TextEdit {
    egui::TextEdit::singleline(text)
        .hint_text(RichText::new("Your key here...").text_style(NotedeckTextStyle::Body.text_style()))
        .vertical_align(Align::Center)
        .min_size(Vec2::new(0.0, 40.0))
        .margin(Margin::same(12.0))
}
