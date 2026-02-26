use crate::processes::TreeRow;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

pub enum ProcessAction {
    Kill(usize),
    Properties(usize),
    ToggleExpand(u32),
}

pub struct ProcessTableResult {
    pub action: Option<ProcessAction>,
    pub clicked_row: Option<usize>,
    pub double_clicked_row: Option<usize>,
    pub hovered_row: Option<usize>,
}

pub fn render_process_table(
    ui: &mut egui::Ui,
    rows: &[TreeRow<'_>],
    selected_row: Option<usize>,
    prev_hovered_row: Option<usize>,
) -> ProcessTableResult {
    let mut action = None;
    let mut clicked_row = None;
    let mut double_clicked_row = None;
    let mut hovered_row = None;

    if rows.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label("No processes. Click \"Refresh\" to reload.");
        });
        return ProcessTableResult {
            action: None,
            clicked_row: None,
            double_clicked_row: None,
            hovered_row: None,
        };
    }

    let available_height = ui.available_height();

    let table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(70.0).at_least(50.0))    // PID
        .column(Column::initial(200.0).at_least(120.0))  // Name (with tree indent)
        .column(Column::initial(180.0).at_least(80.0))   // Product Name
        .column(Column::initial(400.0).at_least(150.0))  // Command Line
        .column(Column::initial(60.0).at_least(45.0))    // CPU %
        .column(Column::initial(80.0).at_least(60.0))    // Memory
        .column(Column::initial(90.0).at_least(60.0))    // Disk Read
        .column(Column::initial(90.0).at_least(60.0))    // Disk Write
        .column(Column::initial(90.0).at_least(60.0))    // Runs As
        .column(Column::initial(75.0).at_least(55.0))    // Visible As
        .column(Column::initial(140.0).at_least(100.0))  // Start Time
        .column(Column::remainder().at_least(160.0))      // Actions
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height);

    table
        .header(20.0, |mut header| {
            header.col(|ui| { ui.strong("PID"); });
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("Product Name"); });
            header.col(|ui| { ui.strong("Command Line"); });
            header.col(|ui| { ui.strong("CPU %"); });
            header.col(|ui| { ui.strong("Memory"); });
            header.col(|ui| { ui.strong("Disk Read"); });
            header.col(|ui| { ui.strong("Disk Write"); });
            header.col(|ui| { ui.strong("Runs As"); });
            header.col(|ui| { ui.strong("Visible As"); });
            header.col(|ui| { ui.strong("Start Time"); });
            header.col(|ui| { ui.strong("Actions"); });
        })
        .body(|body| {
            body.rows(24.0, rows.len(), |mut row| {
                let index = row.index();
                let tree_row = &rows[index];
                let proc = tree_row.process;
                let is_selected = selected_row == Some(index);
                let was_hovered = prev_hovered_row == Some(index);

                if is_selected || was_hovered {
                    row.set_selected(true);
                }

                let mut row_hovered = false;
                let mut row_clicked = false;
                let mut row_double_clicked = false;

                // PID
                let (_, cell_resp) = row.col(|ui| {
                    let label = egui::Label::new(
                        egui::RichText::new(proc.pid.to_string())
                            .color(egui::Color32::from_rgb(180, 180, 180)),
                    )
                    .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Name (with tree lines, expansion boxes, and indentation)
                let (_, cell_resp) = row.col(|ui| {
                    ui.horizontal(|ui| {
                        const INDENT_W: f32 = 18.0;
                        const BOX_SIZE: f32 = 9.0;
                        let line_color = egui::Color32::from_rgb(90, 90, 90);
                        let depth = tree_row.depth;

                        // Total indent area: tree lines + expansion box/spacer
                        let tree_width = depth as f32 * INDENT_W;
                        let box_area_w = BOX_SIZE + 4.0;
                        let total_w = tree_width + box_area_w;

                        // Allocate the tree+box area as one clickable region
                        let (tree_rect, tree_resp) = ui.allocate_exact_size(
                            egui::vec2(total_w, ui.available_height()),
                            if tree_row.has_children { egui::Sense::click() } else { egui::Sense::hover() },
                        );

                        if tree_resp.clicked() && tree_row.has_children {
                            action = Some(ProcessAction::ToggleExpand(proc.pid));
                        }
                        row_hovered |= tree_resp.hovered();

                        let painter = ui.painter();
                        // Compute full row bounds from the known 24.0 row pitch,
                        // centered on the cell. This extends lines past the cell's
                        // content margins so they connect seamlessly between rows.
                        let row_cy = tree_rect.center().y;
                        let row_top = row_cy - 12.0;
                        let row_bottom = row_cy + 12.0;
                        let cell_left = tree_rect.left();

                        // Helper: draw a dotted vertical line
                        let draw_dotted_v = |p: &egui::Painter, x: f32, y1: f32, y2: f32| {
                            let dot_len = 1.5_f32;
                            let gap = 2.0_f32;
                            let stroke = egui::Stroke::new(1.0, line_color);
                            let mut y = y1;
                            while y < y2 {
                                let end = (y + dot_len).min(y2);
                                p.line_segment([egui::pos2(x, y), egui::pos2(x, end)], stroke);
                                y += dot_len + gap;
                            }
                        };

                        // Helper: draw a dotted horizontal line
                        let draw_dotted_h = |p: &egui::Painter, x1: f32, x2: f32, y: f32| {
                            let dot_len = 1.5_f32;
                            let gap = 2.0_f32;
                            let stroke = egui::Stroke::new(1.0, line_color);
                            let mut x = x1;
                            while x < x2 {
                                let end = (x + dot_len).min(x2);
                                p.line_segment([egui::pos2(x, y), egui::pos2(end, y)], stroke);
                                x += dot_len + gap;
                            }
                        };

                        // Draw ancestor vertical connector lines (columns 0..depth-2)
                        for c in 0..depth.saturating_sub(1) {
                            if c < tree_row.connector_lines.len() && tree_row.connector_lines[c] {
                                let x = cell_left + c as f32 * INDENT_W + INDENT_W * 0.5;
                                draw_dotted_v(painter, x, row_top, row_bottom);
                            }
                        }

                        // Draw connector at parent column (depth-1): ├── or └──
                        if depth > 0 {
                            let parent_x = cell_left + (depth - 1) as f32 * INDENT_W + INDENT_W * 0.5;
                            if tree_row.is_last_sibling {
                                // └── corner: vertical top-to-center only
                                draw_dotted_v(painter, parent_x, row_top, row_cy);
                            } else {
                                // ├── tee: vertical top-to-bottom
                                draw_dotted_v(painter, parent_x, row_top, row_bottom);
                            }
                            // Horizontal connector — extend to box for parents, to name for leaves
                            let h_end = cell_left + depth as f32 * INDENT_W
                                + if tree_row.has_children { 0.0 } else { box_area_w };
                            draw_dotted_h(painter, parent_x, h_end, row_cy);
                        }

                        // Draw expansion box [+]/[-] or dot for leaf nodes
                        let box_left = cell_left + depth as f32 * INDENT_W;
                        let box_x = box_left + 2.0;
                        let box_rect = egui::Rect::from_min_size(
                            egui::pos2(box_x, row_cy - BOX_SIZE * 0.5),
                            egui::vec2(BOX_SIZE, BOX_SIZE),
                        );

                        if tree_row.has_children {
                            // Native Windows-style expansion box
                            painter.rect_filled(box_rect, 0.0, egui::Color32::from_rgb(32, 32, 32));
                            painter.rect_stroke(box_rect, 0.0, egui::Stroke::new(1.0, line_color), egui::StrokeKind::Inside);

                            let cx = box_rect.center().x;
                            let cy_box = box_rect.center().y;
                            let sign_color = egui::Color32::from_rgb(180, 180, 180);
                            // Horizontal bar (always present: the minus)
                            painter.line_segment(
                                [egui::pos2(cx - 3.0, cy_box), egui::pos2(cx + 3.0, cy_box)],
                                egui::Stroke::new(1.0, sign_color),
                            );
                            if !tree_row.is_expanded {
                                // Vertical bar (makes it a plus)
                                painter.line_segment(
                                    [egui::pos2(cx, cy_box - 3.0), egui::pos2(cx, cy_box + 3.0)],
                                    egui::Stroke::new(1.0, sign_color),
                                );
                            }

                            // If expanded, draw dotted vertical line from box bottom to row bottom
                            if tree_row.is_expanded {
                                let child_x = cell_left + depth as f32 * INDENT_W + INDENT_W * 0.5;
                                draw_dotted_v(painter, child_x, box_rect.bottom(), row_bottom);
                            }
                        }

                        // Name label
                        let label = egui::Label::new(&proc.name)
                            .truncate()
                            .sense(egui::Sense::click());
                        let resp = ui.add(label);
                        row_hovered |= resp.hovered();
                        row_clicked |= resp.clicked();
                        row_double_clicked |= resp.double_clicked();
                    });
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Product Name
                let (_, cell_resp) = row.col(|ui| {
                    let text = if proc.product_name.is_empty() { "\u{2014}" } else { &proc.product_name };
                    let color = if proc.product_name.is_empty() {
                        egui::Color32::GRAY
                    } else {
                        egui::Color32::from_rgb(200, 200, 200)
                    };
                    let label = egui::Label::new(egui::RichText::new(text).color(color))
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Command Line
                let (_, cell_resp) = row.col(|ui| {
                    let text = if proc.command_line.is_empty() {
                        "\u{2014}"
                    } else {
                        &proc.command_line
                    };
                    let color = if proc.command_line.is_empty() {
                        egui::Color32::GRAY
                    } else {
                        egui::Color32::from_rgb(200, 200, 200)
                    };
                    let label = egui::Label::new(egui::RichText::new(text).color(color))
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // CPU %
                let (_, cell_resp) = row.col(|ui| {
                    let text = if proc.cpu_usage > 0.05 {
                        format!("{:.1}%", proc.cpu_usage)
                    } else {
                        "0%".to_string()
                    };
                    let color = if proc.cpu_usage > 50.0 {
                        egui::Color32::from_rgb(230, 80, 80)
                    } else if proc.cpu_usage > 10.0 {
                        egui::Color32::from_rgb(230, 160, 50)
                    } else {
                        ui.visuals().text_color()
                    };
                    let label = egui::Label::new(egui::RichText::new(&text).color(color))
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Memory
                let (_, cell_resp) = row.col(|ui| {
                    let text = format_memory(proc.memory_bytes);
                    let label = egui::Label::new(&text).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Disk Read
                let (_, cell_resp) = row.col(|ui| {
                    let text = format_bytes(proc.disk_read_bytes);
                    let label = egui::Label::new(&text).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Disk Write
                let (_, cell_resp) = row.col(|ui| {
                    let text = format_bytes(proc.disk_write_bytes);
                    let label = egui::Label::new(&text).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Runs As
                let (_, cell_resp) = row.col(|ui| {
                    let text = if proc.user_name.is_empty() { "--" } else { &proc.user_name };
                    let label = egui::Label::new(text)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Visible As
                let (_, cell_resp) = row.col(|ui| {
                    let (text, color) = if proc.is_elevated {
                        ("Admin", egui::Color32::from_rgb(230, 160, 50))
                    } else {
                        ("User", ui.visuals().text_color())
                    };
                    let label = egui::Label::new(
                        egui::RichText::new(text).color(color),
                    ).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Start Time
                let (_, cell_resp) = row.col(|ui| {
                    let text = match proc.start_time {
                        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                        None => "\u{2014}".to_string(),
                    };
                    let label = egui::Label::new(&text).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Actions: Kill + Properties
                let (_, cell_resp) = row.col(|ui| {
                    ui.horizontal(|ui| {
                        let btn_size = egui::vec2(65.0, 18.0);

                        // Don't allow killing PID 0 or 4 (System)
                        let can_kill = proc.pid > 4;
                        if can_kill {
                            if ui
                                .add_sized(btn_size, egui::Button::new("Kill"))
                                .clicked()
                            {
                                action = Some(ProcessAction::Kill(index));
                            }
                        } else {
                            ui.add_space(btn_size.x + ui.spacing().item_spacing.x);
                        }

                        if ui
                            .add_sized(btn_size, egui::Button::new("Properties"))
                            .clicked()
                        {
                            action = Some(ProcessAction::Properties(index));
                        }
                    });
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                if row_hovered {
                    hovered_row = Some(index);
                }
                if row_clicked {
                    clicked_row = Some(index);
                }
                if row_double_clicked {
                    double_clicked_row = Some(index);
                }
            });
        });

    ProcessTableResult {
        action,
        clicked_row,
        double_clicked_row,
        hovered_row,
    }
}

fn format_memory(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "\u{2014}".to_string()
    } else if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
