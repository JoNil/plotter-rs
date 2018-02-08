extern crate rand;

use std::fs::File;
use std::io::Write;

use rand::distributions::IndependentSample;

fn main() {

    let mut res = String::new();

    let between = rand::distributions::Range::new(-1.0f64, 1.0);
    let mut rng = rand::thread_rng();

    for i in 0..500_000_000 {
        let a = between.ind_sample(&mut rng);

        if i % 1_000_000 < 500_000 {
            res.push_str(&format!("{}\n", a));
        } else {
            res.push_str(&format!("{}\n", 10.0 + a));
        }
    }

    let mut file = File::create("test.txt").unwrap();
    file.write_all(res.as_bytes()).unwrap();
}