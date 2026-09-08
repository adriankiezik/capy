use crate::render::renderer::bench::{self, Gpu, Layer};
use criterion::Criterion;

pub(in crate::render) fn register(criterion: &mut Criterion, gpu: &Gpu) {
    bench::register(criterion, gpu, Layer::Ui, &[16, 64]);
}
