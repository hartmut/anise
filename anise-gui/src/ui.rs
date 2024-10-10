use anise::{almanac::Almanac, errors::AlmanacError};
use eframe::egui;
use egui::Theme;
use egui_extras::{Column, TableBuilder};
use hifitime::TimeScale;

use log::error;
#[cfg(target_arch = "wasm32")]
use poll_promise::Promise;

use crate::{bpc::bpc_ui, spk::spk_ui};

#[cfg(target_arch = "wasm32")]
type AlmanacFile = Option<(String, Vec<u8>)>;

pub struct UiApp {
    pub selected_time_scale: TimeScale,
    pub show_unix: bool,
    pub almanac: Almanac,
    pub path: Option<String>,
    #[cfg(target_arch = "wasm32")]
    promise: Option<Promise<AlmanacFile>>,
}

impl Default for UiApp {
    fn default() -> Self {
        Self {
            selected_time_scale: TimeScale::UTC,
            show_unix: false,
            almanac: Default::default(),
            path: None,
            #[cfg(target_arch = "wasm32")]
            promise: Default::default(),
        }
    }
}

enum FileLoadResult {
    NoFileSelectedYet,
    Ok((String, Almanac)),
    Error(AlmanacError),
}

impl UiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        cc.egui_ctx.set_theme(Theme::Dark);
        Self::default()
    }

    #[cfg(target_arch = "wasm32")]
    fn load_almanac(&mut self) -> FileLoadResult {
        if let Some(promise) = self.promise.as_ref() {
            // We are already waiting for a file, so we don't need to show the dialog again
            if let Some(result) = promise.ready() {
                let (file_name, data) = result.as_ref().map(|x| x.clone()).unwrap();
                self.promise = None;
                match self.almanac.load_from_bytes(bytes::Bytes::from(data)) {
                    Ok(almanac) => FileLoadResult::Ok((file_name, almanac)),
                    Err(e) => FileLoadResult::Error(e),
                }
            } else {
                FileLoadResult::NoFileSelectedYet
            }
        } else {
            // Show the dialog and start loading the file
            self.promise = Some(Promise::spawn_local(async move {
                let fh = rfd::AsyncFileDialog::new().pick_file().await?;
                Some((fh.file_name(), fh.read().await))
            }));
            FileLoadResult::NoFileSelectedYet
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_almanac(&mut self) -> FileLoadResult {
        if let Some(path_buf) = rfd::FileDialog::new().pick_file() {
            let path = path_buf.to_str().unwrap().to_string();
            match self.almanac.load(&path) {
                Ok(almanac) => FileLoadResult::Ok((path, almanac)),
                Err(e) => FileLoadResult::Error(e),
            }
        } else {
            FileLoadResult::NoFileSelectedYet
        }
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_pixels_per_point(1.25);

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("ANISE v0.4");
                    ui.label("A modern rewrite of NASA's SPICE toolkit");
                    ui.hyperlink_to("Contact", "https://7ug5imdtt8v.typeform.com/to/neFvVW3p");
                    ui.hyperlink_to(
                        "https://www.nyxspace.com",
                        "https://www.nyxspace.com?utm_source=gui",
                    );
                });
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .min_height(0.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Run log");
                });

                egui_logger::LoggerUi::default()
                    .enable_ctx_menu(false)
                    .enable_regex(false)
                    .show(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.vertical_centered(|ui| {
                        match &self.path {
                            None => {
                                let mut trigger_file_load = false;
                                trigger_file_load |= ui.button("Select file to inspect...").clicked();

                                // If we are in the browser, we need to also check if the file
                                // is ready to be loaded instead of just checking if the button
                                // was clicked
                                #[cfg(target_arch = "wasm32")]
                                {
                                    trigger_file_load |= self.promise.is_some();
                                }

                                // Show the open file dialog
                                if trigger_file_load {
                                        // Try to load this file
                                        match self.load_almanac() {
                                            FileLoadResult::NoFileSelectedYet => {
                                            }
                                            FileLoadResult::Ok((path, almanac)) => {
                                                self.almanac = almanac;
                                                self.path = Some(path);
                                            }
                                            FileLoadResult::Error(e) => {
                                                error!("{e}");
                                            }
                                        }
                                }
                            }
                            Some(path) => {
                                // Grab generic data
                                let (label, crc) = if self.almanac.num_loaded_spk() == 1 {
                                    (
                                        "DAF/SPK",
                                        self.almanac.spk_data[0].as_ref().unwrap().crc32(),
                                    )
                                } else if self.almanac.num_loaded_bpc() == 1 {
                                    (
                                        "DAF/PCK",
                                        self.almanac.bpc_data[0].as_ref().unwrap().crc32(),
                                    )
                                } else if !self.almanac.planetary_data.is_empty() {
                                    ("ANISE/PCA", self.almanac.planetary_data.crc32())
                                } else if !self.almanac.spacecraft_data.is_empty() {
                                    ("ANISE/SCA", self.almanac.spacecraft_data.crc32())
                                } else if !self.almanac.euler_param_data.is_empty() {
                                    ("ANISE/EPA", self.almanac.euler_param_data.crc32())
                                } else {
                                    ("UNKNOWN", 0)
                                };

                                let mut unload_file = false;
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui|{
                                        ui.label(format!("Inspecting {path}"));
                                        if ui.button("Close").clicked() {
                                            unload_file = true;
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("File type");
                                        ui.label(label);

                                        ui.label("CRC32");
                                        ui.text_edit_singleline(&mut format!("{crc}"));

                                        if label.ends_with("SPK") {
                                            let num_summaries = self.almanac.spk_domains().unwrap().len();
                                            ui.label("Number of summaries");
                                            ui.label(format!("{num_summaries}"));
                                        } else if label.ends_with("BPC") {
                                            let num_summaries = self.almanac.bpc_domains().unwrap().len();
                                            ui.label("Number of summaries");
                                            ui.label(format!("{num_summaries}"));
                                        }
                                    });

                                    if label.starts_with("DAF/") {
                                        ui.horizontal(|ui| {
                                            ui.label("Time scale");
                                            egui::ComboBox::new("attention", "")
                                                .selected_text(format!(
                                                    "{}",
                                                    self.selected_time_scale
                                                ))
                                                .show_ui(ui, |ui| {
                                                    for ts in [
                                                        TimeScale::UTC,
                                                        TimeScale::ET,
                                                        TimeScale::TDB,
                                                        TimeScale::TAI,
                                                        TimeScale::TT,
                                                    ] {
                                                        ui.selectable_value(
                                                            &mut self.selected_time_scale,
                                                            ts,
                                                            format!("{ts}"),
                                                        );
                                                    }
                                                });

                                            ui.checkbox(
                                                &mut self.show_unix,
                                                "UNIX timestamps",
                                            );
                                        });
                                    }

                                    // Now display the data
                                    if label == "DAF/PCK" {
                                        bpc_ui(ui, &self.almanac, self.show_unix, self.selected_time_scale);
                                    } else if label == "DAF/SPK" {
                                        spk_ui(ui, &self.almanac, self.show_unix, self.selected_time_scale)
                                    } else if label == "ANISE/PCA" {
                                        TableBuilder::new(ui)
                                            .column(Column::auto().at_least(100.0).resizable(true))
                                            .column(Column::auto().at_least(50.0).resizable(true))
                                            .column(Column::auto().at_least(75.0).resizable(true))
                                            .column(Column::auto().at_least(75.0).resizable(true))
                                            .column(Column::auto().at_least(75.0).resizable(true))
                                            .column(Column::auto().at_least(125.0).resizable(true))
                                            .column(Column::auto().at_least(125.0).resizable(true))
                                            .column(Column::auto().at_least(125.0).resizable(true))
                                            .column(Column::remainder())
                                            .header(20.0, |mut header| {
                                                header.col(|ui| {
                                                    ui.heading("Name");
                                                });
                                                header.col(|ui| {
                                                    ui.heading("ID");
                                                });
                                                header.col(|ui| {
                                                    ui.heading("Gravity param (km^3/s^2)");
                                                });

                                                header.col(|ui| {
                                                    ui.heading("Major axis (km)");
                                                });
                                                header.col(|ui| {
                                                    ui.heading("Minor axis (km)");
                                                });
                                                header.col(|ui| {
                                                    ui.heading("Polar axis (km)");
                                                });

                                                header.col(|ui| {
                                                    ui.heading("Pole right asc.");
                                                });
                                                header.col(|ui| {
                                                    ui.heading("Pole declination");
                                                });
                                                header.col(|ui| {
                                                    ui.heading("Prime meridian");
                                                });
                                            })
                                            .body(|mut body| {
                                                let pck = &self.almanac.planetary_data;

                                                let binding = pck.lut.entries();
                                                let mut values = binding.values().collect::<Vec<_>>().to_vec();
                                                values.sort_by_key(|(opt_id, _)| match opt_id {
                                                    Some(id) => *id,
                                                    None => 0
                                                });

                                                for (opt_id, opt_name) in values
                                                {
                                                    let data = if let Some(id) = opt_id {
                                                        pck.get_by_id(*id).unwrap()
                                                    } else {
                                                        pck.get_by_name(&opt_name.clone().unwrap()).unwrap()
                                                    };

                                                    body.row(30.0, |mut row| {
                                                        row.col(|ui| {
                                                            ui.label(match opt_name {
                                                                Some(name) => format!("{name}"),
                                                                None => "Unset".to_string(),
                                                            });
                                                        });

                                                        row.col(|ui| {
                                                            ui.label(match opt_id {
                                                                Some(id) => format!("{id}"),
                                                                None => "Unset".to_string(),
                                                            });
                                                        });

                                                        row.col(|ui| {
                                                            ui.text_edit_singleline(&mut format!(
                                                                "{}",
                                                                data.mu_km3_s2
                                                            ));
                                                        });

                                                        match data.shape {
                                                            None => {
                                                                // Three empty columns
                                                                row.col(|ui| {
                                                                    ui.label("Unset");
                                                                });
                                                                row.col(|ui| {
                                                                    ui.label("Unset");
                                                                });
                                                                row.col(|ui| {
                                                                    ui.label("Unset");
                                                                });
                                                            }
                                                            Some(shape) => {
                                                                row.col(|ui| {
                                                                    ui.text_edit_singleline(
                                                                        &mut format!(
                                                                            "{}",
                                                                            shape.semi_major_equatorial_radius_km
                                                                        ),
                                                                    );
                                                                });
                                                                row.col(|ui| {
                                                                    ui.text_edit_singleline(
                                                                        &mut format!(
                                                                            "{}",
                                                                            shape.semi_minor_equatorial_radius_km
                                                                        ),
                                                                    );
                                                                });
                                                                row.col(|ui| {
                                                                    ui.text_edit_singleline(
                                                                        &mut format!(
                                                                            "{}",
                                                                            shape.polar_radius_km
                                                                        ),
                                                                    );
                                                                });
                                                            }
                                                        }

                                                        match data.pole_right_ascension {
                                                            None => row.col(|ui| {
                                                                ui.label("Unset");
                                                            }),
                                                            Some(pole_ra) => {
                                                                row.col(|ui| {
                                                                    ui.label(format!("{pole_ra}"));
                                                                })
                                                            }
                                                        };

                                                        match data.pole_declination {
                                                            None => row.col(|ui| {
                                                                ui.label("Unset");
                                                            }),
                                                            Some(pole_dec) => {
                                                                row.col(|ui| {
                                                                    ui.label(format!("{pole_dec}"));
                                                                })
                                                            }
                                                        };

                                                        match data.prime_meridian {
                                                            None => row.col(|ui| {
                                                                ui.label("Unset");
                                                            }),
                                                            Some(pm) => {
                                                                row.col(|ui| {
                                                                    ui.label(format!("{pm}"));
                                                                })
                                                            }
                                                        };
                                                    });
                                                }
                                            });
                                    }
                                });

                                if unload_file {
                                    self.almanac = Almanac::default();
                                    self.path = None;
                                }
                            }
                        };
                    });
                });
            });
        });
    }
}
