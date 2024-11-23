use std::env;

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use plotters::prelude::*;
use reqwest;
use serde_json::{json, Value};

#[derive(Debug)]
struct CandleData {
    time_idx: i32,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    time: DateTime<Utc>,
}

async fn get_current_price(product_id: &str) -> Result<f64> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.exchange.coinbase.com/products/{}/ticker",
        product_id
    );

    let response = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;

    let data: serde_json::Value = response.json().await?;

    // Extract the price from the response
    let price = data["price"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Price not found in response"))?
        .parse::<f64>()?;

    Ok(price)
}

async fn get_candle_data(symbol: &str) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();
    // Get current time for end parameter
    let end = Utc::now();
    let start = end - chrono::Duration::hours(48);
    let url = format!(
        "https://api.exchange.coinbase.com/products/{symbol}-USD/candles\
        ?start={}\
        &end={}\
        &granularity=3600",
        start.timestamp(),
        end.timestamp()
    );

    let response = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;

    // let data: Vec<Vec<f64>> = response.json().await?;

    let data = response.text().await?;

    Ok(data)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the logger with pretty print
    tracing_subscriber::fmt()
        .pretty()
        // sets this to be the default, global collector for this application.
        .init();

    let eth_market_data = get_candle_data("ETH").await?;

    let current_price = get_current_price("ETH-USD").await?;
    tracing::info!("Current ETH price: ${:.2}", current_price);

    // save text to a file
    std::fs::write("response.json", &eth_market_data)?;

    let data: Vec<Vec<f64>> = serde_json::from_str(&eth_market_data)?;

    // Reverse the data as Coinbase returns it in descending order
    let candles: Vec<CandleData> = data
        .iter()
        .rev() // Reverse the order to get ascending
        .enumerate()
        .map(|(i, kline)| CandleData {
            time_idx: i as i32,
            time: DateTime::from_timestamp(kline[0] as i64, 0).unwrap(),
            open: kline[3],
            high: kline[2],
            low: kline[1],
            close: kline[4],
            volume: kline[5],
        })
        .collect();

    // Rest of the code remains exactly the same...
    let root = BitMapBackend::new("eth_chart.jpg", (1200, 900)).into_drawing_area();
    root.fill(&RGBColor(27, 27, 27))?;

    let (price_area, volume_area) = root.split_vertically(600);

    let min_price = candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let max_price = candles
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let price_range = max_price - min_price;
    let y_range = (
        min_price - price_range * 0.05,
        max_price + price_range * 0.05,
    );

    let max_volume = candles.iter().map(|c| c.volume).fold(0.0, f64::max);

    let mut price_chart = ChartBuilder::on(&price_area)
        .caption("ETH/USD", ("sans-serif", 30).into_font().color(&WHITE))
        .margin(10)
        .x_label_area_size(30)
        .y_label_area_size(60)
        .build_cartesian_2d(-1.0f32..candles.len() as f32, y_range.0..y_range.1)?;

    price_chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .light_line_style(&RGBColor(45, 45, 45))
        .bold_line_style(&RGBColor(45, 45, 45))
        .y_desc("Price (USD)")
        .axis_desc_style(("sans-serif", 15).into_font().color(&WHITE))
        .label_style(("sans-serif", 12).into_font().color(&WHITE))
        .x_label_formatter(&|x| {
            if *x >= 0.0 && (*x as usize) < candles.len() {
                candles[*x as usize].time.format("%H:%M").to_string()
            } else {
                String::new()
            }
        })
        .draw()?;

    let mut volume_chart = ChartBuilder::on(&volume_area)
        .margin(10)
        .x_label_area_size(30)
        .y_label_area_size(60)
        .build_cartesian_2d(-1.0f32..candles.len() as f32, 0f64..max_volume)?;

    volume_chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .light_line_style(&RGBColor(45, 45, 45))
        .bold_line_style(&RGBColor(45, 45, 45))
        .y_desc("Volume")
        .x_desc("Time")
        .axis_desc_style(("sans-serif", 15).into_font().color(&WHITE))
        .label_style(("sans-serif", 12).into_font().color(&WHITE))
        .x_label_formatter(&|x| {
            if *x >= 0.0 && (*x as usize) < candles.len() {
                candles[*x as usize].time.format("%H:%M").to_string()
            } else {
                String::new()
            }
        })
        .draw()?;

    // Draw grid lines
    for i in 0..candles.len() {
        if i % 12 == 0 {
            // Grid line every hour
            let x = i as f32;
            price_chart.draw_series(std::iter::once(PathElement::new(
                vec![(x, y_range.0), (x, y_range.1)],
                &RGBColor(45, 45, 45),
            )))?;
            volume_chart.draw_series(std::iter::once(PathElement::new(
                vec![(x, 0.0), (x, max_volume)],
                &RGBColor(45, 45, 45),
            )))?;
        }
    }

    // Draw candles and volume
    for candle in &candles {
        let x = candle.time_idx as f32;
        let bar_color = if candle.close >= candle.open {
            &RGBColor(0, 255, 0)
        } else {
            &RGBColor(255, 0, 0)
        };

        // Draw price candle
        price_chart.draw_series(std::iter::once(Rectangle::new(
            [
                (x - 0.4, candle.open.min(candle.close)),
                (x + 0.4, candle.open.max(candle.close)),
            ],
            bar_color.filled(),
        )))?;

        // Draw wicks
        price_chart.draw_series(std::iter::once(PathElement::new(
            vec![(x, candle.high), (x, candle.open.max(candle.close))],
            bar_color,
        )))?;

        price_chart.draw_series(std::iter::once(PathElement::new(
            vec![(x, candle.low), (x, candle.open.min(candle.close))],
            bar_color,
        )))?;

        // Draw volume bar
        volume_chart.draw_series(std::iter::once(Rectangle::new(
            [(x - 0.4, 0.0), (x + 0.4, candle.volume)],
            bar_color.mix(0.3).filled(),
        )))?;
    }

    root.present()?;

    // Copy to clipboard
    let img = image::open("eth_chart.jpg")?;
    let mut clipboard = arboard::Clipboard::new()?;
    // clear the clipboard
    clipboard.set_text("")?;
    clipboard.set_image(arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: img.to_rgba8().into_raw().into(),
    })?;
    // sleep for 10 milliseconds to try to let the clipboard catch up
    std::thread::sleep(std::time::Duration::from_millis(10));

    // tracing::info!("Chart has been saved as 'eth_chart.jpg'");

    analyze_data(&eth_market_data, current_price).await?;

    Ok(())
}

async fn analyze_data(market_data: &str, current_price: f64) -> Result<()> {
    // Load environment variables
    dotenvy::dotenv().expect("Failed to load .env file");

    // Initialize OpenAI client
    let api_key = env::var("OPENAI_API_KEY")?;

    // Read and encode the image
    let image_path = "eth_chart.jpg";
    let image_data = std::fs::read(image_path)?;
    let base64_image = general_purpose::STANDARD.encode(image_data);

    // Read system prompt
    let system_prompt = std::fs::read_to_string("prompt.txt")?;

    let base64_url = format!("data:image/jpeg;base64,{}", base64_image);

    // get current time in epoch seconds
    let current_time = Utc::now().timestamp();
    tracing::info!("Current time: {}", current_time);

    let eth_market_data = json!({"eth_candles" : market_data});

    let btc_candles = get_candle_data("BTC").await?;

    let btc_market_data = json!({"btc_candles" : btc_candles});

    // Construct the request body
    let body = json!({
        "model": "o1-preview",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": system_prompt
                    },
                    {
                        "type": "text",
                        "text": eth_market_data.to_string()
                    },
                    {
                        "type": "text",
                        "text": btc_market_data.to_string()
                    },
                    {
                        "type": "text",
                        "text": format!("current time is {current_time}")
                    },
                    // {
                    //     "type": "text",
                    //     "text": format!("current price is {current_price}")
                    // },
                    // {
                    //     "type": "image_url",
                    //     "image_url": {
                    //         "url": base64_url
                    //     }
                    // }
                ]
            }
        ],
        // "max_tokens": 300
    });

    // Make the API request
    let client = reqwest::Client::new();

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(60 * 5))
        .json(&body)
        .send()
        .await?;

    // Handle the response
    let response_body: Value = response.json().await?;

    // Extract and print the response content
    if let Some(content) = response_body["choices"][0]["message"]["content"].as_str() {
        tracing::info!("{content}");
    } else {
        tracing::info!("Unexpected response format: {:?}", response_body);
    }

    Ok(())
}
