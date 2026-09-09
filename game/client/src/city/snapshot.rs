use super::City;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

impl City {
    pub fn save_legacy_snapshot(&self, path: &Path) -> anyhow::Result<()> {
        let mut out = BufWriter::new(File::create(path)?);

        out.write_all(b"CAPYBENCH\x02")?;

        let camera = self.camera;

        for value in camera
            .position
            .to_array()
            .into_iter()
            .chain([
                camera.direction.z.atan2(camera.direction.x),
                camera.direction.y.asin(),
                0.0,
                camera.fov_radians,
                camera.near_plane,
                600.0,
            ])
            .chain([0.0; 32])
        {
            out.write_all(&value.to_le_bytes())?;
        }

        out.write_all(&(self.instances.len() as u32).to_le_bytes())?;

        let encoded: Vec<Vec<u8>> = self
            .assets
            .iter()
            .map(|asset| {
                let mut bytes = Vec::new();

                let mut last = 0u8;

                let mut count = 0u16;

                for z in 0..asset.size.z {
                    for y in 0..asset.size.y {
                        for x in 0..asset.size.x {
                            let material = asset
                                .sample(engine::IVec3::new(x, y, z))
                                .map_or(0, |index| [8, 3, 12, 20][index]);

                            if count != 0 && (material != last || count == u16::MAX) {
                                bytes.push(last);

                                bytes.extend_from_slice(&count.to_le_bytes());

                                count = 0;
                            }

                            last = material;
                            count += 1;
                        }
                    }
                }

                if count != 0 {
                    bytes.push(last);

                    bytes.extend_from_slice(&count.to_le_bytes());
                }

                bytes
            })
            .collect();

        for (instance, &index) in self.instances.iter().zip(&self.asset_indices) {
            out.write_all(&(instance.id as u32).to_le_bytes())?;

            out.write_all(&[1])?;

            out.write_all(&(instance.scale * 0.1).to_le_bytes())?;

            for v in self.assets[index].size.to_array() {
                out.write_all(&v.to_le_bytes())?;
            }

            for v in instance
                .translation
                .to_array()
                .into_iter()
                .chain(instance.rotation.to_array())
            {
                out.write_all(&v.to_le_bytes())?;
            }

            for _ in 0..5 {
                out.write_all(&0i32.to_le_bytes())?;
            }

            out.write_all(&(encoded[index].len() as u32).to_le_bytes())?;

            out.write_all(&encoded[index])?;
        }

        out.flush()?;

        Ok(())
    }
}
