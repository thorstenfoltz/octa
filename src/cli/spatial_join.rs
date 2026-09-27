//! `octa --spatial-join POINTS --spatial-layer FILE... [--spatial-op
//! inside|nearest] [--within-km N]` - join by location: the polygon each
//! point lies in, or the nearest point of each layer with its distance.

use std::path::PathBuf;

use octa::data::spatial_join::{Layer, SpatialOp, point_cols, prefix_for, spatial_join};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub points: PathBuf,
    pub layers: Vec<PathBuf>,
    pub nearest: bool,
    pub within_km: Option<f64>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    anyhow::ensure!(
        !args.layers.is_empty(),
        "--spatial-join needs at least one --spatial-layer"
    );
    let points = super::read_table(&args.points)?;
    let cols = point_cols(&points).ok_or_else(|| {
        anyhow::anyhow!(
            "{} has no latitude/longitude columns and no point geometry",
            args.points.display()
        )
    })?;
    let tables = args
        .layers
        .iter()
        .map(|p| {
            Ok((
                prefix_for(&p.file_name().unwrap_or_default().to_string_lossy()),
                super::read_table(p)?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let layers: Vec<Layer> = tables
        .iter()
        .map(|(name, table)| Layer {
            name: name.clone(),
            table,
        })
        .collect();
    let op = if args.nearest {
        SpatialOp::Nearest {
            within_km: args.within_km,
        }
    } else {
        SpatialOp::Inside
    };
    let r = spatial_join(&points, cols, &layers, op)?;
    write_table(&r.table, format)?;
    if r.multi_match > 0 {
        eprintln!(
            "note: {} point(s) lay in more than one polygon of a layer; the first was used",
            r.multi_match
        );
    }
    if r.no_point > 0 {
        eprintln!("note: {} row(s) have no readable point", r.no_point);
    }
    Ok(())
}
