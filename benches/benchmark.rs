use async_net::{Ipv4Addr, SocketAddrV4};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use easy_parallel::Parallel;
use geph_nat::GephNat;

pub fn big_group(c: &mut Criterion) {
    let nat = GephNat::new(
        100,
        Ipv4Addr::new(
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(..),
        ),
    );

    let mut group = c.benchmark_group("big_group");

    let src_skt = SocketAddrV4::new(
        Ipv4Addr::new(
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(..),
        ),
        1100,
    );

    let dest_skt = SocketAddrV4::new(
        Ipv4Addr::new(
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(..),
        ),
        80,
    );

    let id_up = "up_single";
    group.bench_function(id_up, |b| {
        b.iter(|| nat.rewrite_upstream_src(black_box(src_skt), black_box(dest_skt)))
    });
    let id_down = "down_single";
    group.bench_function(id_down, |b| {
        b.iter(|| nat.rewrite_downstream_dest(black_box(dest_skt), black_box(src_skt)))
    });

    group.bench_function("up_multi", |b| {
        b.iter(|| {
            const THREADS: usize = 2;
            const ITERS: usize = 100000;
            Parallel::new()
                .each(0..THREADS, |_| {
                    let src_skt = SocketAddrV4::new(
                        Ipv4Addr::new(
                            fastrand::u8(..),
                            fastrand::u8(..),
                            fastrand::u8(..),
                            fastrand::u8(..),
                        ),
                        1100,
                    );

                    let dest_skt = SocketAddrV4::new(
                        Ipv4Addr::new(
                            fastrand::u8(..),
                            fastrand::u8(..),
                            fastrand::u8(..),
                            fastrand::u8(..),
                        ),
                        80,
                    );

                    for _ in 0..ITERS {
                        black_box(
                            nat.rewrite_upstream_src(black_box(src_skt), black_box(dest_skt)),
                        );
                    }
                })
                .run()
        })
    });
}

criterion_group!(benches, big_group);
criterion_main!(benches);
