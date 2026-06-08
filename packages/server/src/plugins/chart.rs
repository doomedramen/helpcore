pub fn generate_svg(
    chart_type: &str,
    labels: &[String],
    values: &[f64],
    title: Option<&str>,
    colors: Option<Vec<String>>,
    width: u32,
    height: u32,
) -> String {
    if labels.len() != values.len() {
        return String::new();
    }

    let mut svg = format!(
        r#"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg" style="background: white; font-family: sans-serif;">"#,
        width, height, width, height
    );

    // Default colors
    let default_colors = vec![
        "#3498db".to_string(),
        "#e74c3c".to_string(),
        "#2ecc71".to_string(),
        "#f1c40f".to_string(),
        "#9b59b6".to_string(),
        "#1abc9c".to_string(),
        "#e67e22".to_string(),
        "#34495e".to_string(),
    ];
    let active_colors = colors.unwrap_or(default_colors);

    // Title
    if let Some(t) = title {
        let escaped = xml_escape(t);
        svg.push_str(&format!(
            r##"<text x="{}" y="35" text-anchor="middle" font-size="20" font-weight="bold" fill="#333">{}</text>"##,
            width / 2,
            escaped
        ));
    }

    match chart_type {
        "bar" => render_bar(&mut svg, labels, values, &active_colors, width, height),
        "pie" | "donut" => render_pie(
            &mut svg,
            labels,
            values,
            &active_colors,
            width,
            height,
            chart_type == "donut",
        ),
        "line" => render_line(&mut svg, labels, values, &active_colors, width, height),
        _ => {}
    }

    svg.push_str("</svg>");
    svg
}

fn render_bar(
    svg: &mut String,
    labels: &[String],
    values: &[f64],
    colors: &[String],
    width: u32,
    height: u32,
) {
    let margin = 60;
    let chart_width = width as i32 - margin * 2;
    let chart_height = height as i32 - margin * 2;
    if chart_width <= 0 || chart_height <= 0 || values.is_empty() {
        return;
    }

    let max_val = values.iter().fold(0.0_f64, |a, &b| a.max(b));
    let bar_width = (chart_width as f64 / values.len() as f64) * 0.8;
    let gap = (chart_width as f64 / values.len() as f64) * 0.2;

    for (i, (&val, label)) in values.iter().zip(labels).enumerate() {
        let h = if max_val > 0.0 {
            (val / max_val) * chart_height as f64
        } else {
            0.0
        };
        let x = margin as f64 + i as f64 * (bar_width + gap) + gap / 2.0;
        let y = height as f64 - margin as f64 - h;
        let color = &colors[i % colors.len()];

        svg.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" />"#,
            x, y, bar_width, h, color
        ));
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="middle" font-size="12" fill="#666" transform="rotate(45, {}, {})">{}</text>"##,
            x + bar_width / 2.0,
            height as f64 - margin as f64 + 15.0,
            x + bar_width / 2.0,
            height as f64 - margin as f64 + 15.0,
            xml_escape(label)
        ));
    }
}

fn render_pie(
    svg: &mut String,
    labels: &[String],
    values: &[f64],
    colors: &[String],
    width: u32,
    height: u32,
    donut: bool,
) {
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0 + 10.0;
    let radius = (width.min(height) as f64 / 2.0) * 0.65;
    let mut current_angle: f64 = 0.0;

    for (i, (&val, label)) in values.iter().zip(labels).enumerate() {
        let slice_angle = (val / total) * 2.0 * std::f64::consts::PI;
        let x1 = cx + radius * current_angle.cos();
        let y1 = cy + radius * current_angle.sin();
        let x2 = cx + radius * (current_angle + slice_angle).cos();
        let y2 = cy + radius * (current_angle + slice_angle).sin();
        let large_arc = if slice_angle > std::f64::consts::PI {
            1
        } else {
            0
        };
        let color = &colors[i % colors.len()];

        svg.push_str(&format!(
            r#"<path d="M {} {} L {} {} A {} {} 0 {} 1 {} {} Z" fill="{}" />"#,
            cx, cy, x1, y1, radius, radius, large_arc, x2, y2, color
        ));

        // Labels
        let mid_angle = current_angle + slice_angle / 2.0;
        let tx = cx + (radius + 25.0) * mid_angle.cos();
        let ty = cy + (radius + 25.0) * mid_angle.sin();
        let anchor = if mid_angle.cos() > 0.0 {
            "start"
        } else {
            "end"
        };
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="{}" font-size="11" fill="#333">{} ({:.1}%)</text>"##,
            tx,
            ty,
            anchor,
            xml_escape(label),
            (val / total) * 100.0
        ));

        current_angle += slice_angle;
    }

    if donut {
        svg.push_str(&format!(
            r#"<circle cx="{}" cy="{}" r="{}" fill="white" />"#,
            cx,
            cy,
            radius * 0.5
        ));
    }
}

fn render_line(
    svg: &mut String,
    labels: &[String],
    values: &[f64],
    colors: &[String],
    width: u32,
    height: u32,
) {
    let margin = 60;
    let chart_width = width as i32 - margin * 2;
    let chart_height = height as i32 - margin * 2;
    if chart_width <= 0 || chart_height <= 0 || values.len() < 2 {
        return;
    }

    let max_val = values.iter().fold(0.0_f64, |a, &b| a.max(b));
    let step_x = chart_width as f64 / (values.len() - 1) as f64;
    let color = &colors[0];

    let mut path_data = String::new();
    for (i, &val) in values.iter().enumerate() {
        let x = margin as f64 + i as f64 * step_x;
        let y = if max_val > 0.0 {
            height as f64 - margin as f64 - (val / max_val) * chart_height as f64
        } else {
            height as f64 - margin as f64
        };
        if i == 0 {
            path_data.push_str(&format!("M {} {}", x, y));
        } else {
            path_data.push_str(&format!(" L {} {}", x, y));
        }
        svg.push_str(&format!(
            r#"<circle cx="{}" cy="{}" r="4" fill="{}" />"#,
            x, y, color
        ));
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="middle" font-size="11" fill="#666" transform="rotate(45, {}, {})">{}</text>"##,
            x,
            height as f64 - margin as f64 + 15.0,
            x,
            height as f64 - margin as f64 + 15.0,
            xml_escape(&labels[i])
        ));
    }
    svg.push_str(&format!(
        r#"<path d="{}" fill="none" stroke="{}" stroke-width="3" />"#,
        path_data, color
    ));
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bar_svg() {
        let svg = generate_svg(
            "bar",
            &["A".to_string(), "B".to_string()],
            &[10.0, 20.0],
            Some("Test Chart"),
            None,
            600,
            400,
        );
        assert!(svg.contains("svg"));
        assert!(svg.contains("Test Chart"));
        assert!(svg.contains("rect"));
    }

    #[test]
    fn test_generate_pie_svg() {
        let svg = generate_svg(
            "pie",
            &["A".to_string(), "B".to_string()],
            &[10.0, 20.0],
            None,
            None,
            600,
            400,
        );
        assert!(svg.contains("path"));
    }

    #[test]
    fn test_generate_line_svg() {
        let svg = generate_svg(
            "line",
            &["Jan".to_string(), "Feb".to_string(), "Mar".to_string()],
            &[10.0, 20.0, 15.0],
            Some("Growth"),
            None,
            600,
            400,
        );
        assert!(svg.contains("svg"));
        assert!(svg.contains("Growth"));
        assert!(svg.contains("path"));
        assert!(svg.contains("circle"));
    }

    #[test]
    fn test_generate_donut_svg() {
        let svg = generate_svg(
            "donut",
            &["A".to_string(), "B".to_string()],
            &[10.0, 20.0],
            None,
            None,
            600,
            400,
        );
        assert!(svg.contains("svg"));
        assert!(svg.contains("path"));
        assert!(svg.contains("circle"));
    }

    #[test]
    fn test_escapes_label_xml() {
        let svg = generate_svg(
            "pie",
            &["A < B".to_string(), "C & D".to_string()],
            &[10.0, 20.0],
            Some("Title < & >"),
            None,
            600,
            400,
        );
        assert!(svg.contains("A &lt; B"));
        assert!(svg.contains("C &amp; D"));
        assert!(svg.contains("Title &lt; &amp; &gt;"));
    }

    #[test]
    fn test_mismatched_lengths_returns_empty() {
        let svg = generate_svg(
            "bar",
            &["A".to_string()],
            &[10.0, 20.0],
            None,
            None,
            600,
            400,
        );
        assert!(svg.is_empty());
    }

    #[test]
    fn test_empty_data() {
        let svg = generate_svg("bar", &[] as &[String], &[] as &[f64], None, None, 600, 400);
        assert!(svg.contains("svg"));
        assert!(!svg.contains("<rect"));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(
            xml_escape("a < b & c > d \"e'f"),
            "a &lt; b &amp; c &gt; d &quot;e&apos;f"
        );
        assert_eq!(xml_escape("plain text"), "plain text");
        assert_eq!(xml_escape(""), "");
    }
}
