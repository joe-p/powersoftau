extern crate powersoftau;
extern crate bellman;

use powersoftau::small_bls12_381::Bls12CeremonyParameters;
use powersoftau::parameters::PowersOfTauParameters;
use bellman::pairing::bls12_381::{G1Affine, G2Affine};
use bellman::pairing::*;
use std::fs::OpenOptions;
use std::io::{BufReader, BufWriter, Read, Write};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: {} <input_challenge> <output_challenge> <target_power>", args[0]);
        eprintln!("Example: {} challenge_2_28 challenge_2_20 20", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];
    let output_file = &args[2];
    let target_power: usize = args[3].parse().expect("target_power must be a valid number");

    if target_power > 27 {
        eprintln!("Error: target_power cannot be greater than 27 (current maximum)");
        std::process::exit(1);
    }

    println!("Reducing challenge from input file '{}' to target power 2^{} in output file '{}'", 
             input_file, target_power, output_file);

    let parameters = Bls12CeremonyParameters{};
    
    // Calculate target lengths
    let target_tau_powers_length = 1 << target_power;
    let target_tau_powers_g1_length = (target_tau_powers_length << 1) - 1;

    // Detect input file size to determine current power
    let input_file_size = std::fs::metadata(input_file)
        .expect("unable to get input file metadata")
        .len() as usize;
    
    // Calculate current power from file size
    // File format: 64-byte hash + accumulator data
    let accumulator_size = input_file_size - 64;
    let current_power = detect_power_from_size(accumulator_size, &parameters);
    let current_tau_powers_length = 1 << current_power;
    let current_tau_powers_g1_length = (current_tau_powers_length << 1) - 1;
    
    println!("Detected input challenge size:");
    println!("  Current power: 2^{} (tau_powers_length: {})", current_power, current_tau_powers_length);
    println!("  tau_powers_g1 length: {}", current_tau_powers_g1_length);
    println!("  File size: {} bytes", input_file_size);
    println!();

    if target_power > current_power {
        eprintln!("Error: target power 2^{} is larger than input power 2^{}", target_power, current_power);
        std::process::exit(1);
    }

    println!("Target accumulator size:");
    println!("  tau_powers_g1: {} -> {}", current_tau_powers_g1_length, target_tau_powers_g1_length);
    println!("  tau_powers_g2: {} -> {}", current_tau_powers_length, target_tau_powers_length);
    println!("  alpha_tau_powers_g1: {} -> {}", current_tau_powers_length, target_tau_powers_length);
    println!("  beta_tau_powers_g1: {} -> {}", current_tau_powers_length, target_tau_powers_length);

    // Open files
    let input = OpenOptions::new()
        .read(true)
        .open(input_file)
        .expect(&format!("unable to open input file '{}'", input_file));
    
    let mut input = BufReader::new(input);

    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_file)
        .expect(&format!("unable to create output file '{}'", output_file));

    let mut output = BufWriter::new(output);

    // Copy the 64-byte hash
    let mut hash = [0u8; 64];
    input.read_exact(&mut hash).expect("unable to read hash from input file");
    output.write_all(&hash).expect("unable to write hash to output file");

    // Stream copy tau_powers_g1 (first target_tau_powers_g1_length points)
    println!("Copying tau_powers_g1...");
    stream_copy_points::<G1Affine, _>(&mut input, &mut output, target_tau_powers_g1_length, Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE)?;
    
    // Skip remaining tau_powers_g1 points
    let skip_g1_points = current_tau_powers_g1_length - target_tau_powers_g1_length;
    if skip_g1_points > 0 {
        skip_points(&mut input, skip_g1_points, Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE)?;
    }

    // Stream copy tau_powers_g2 (first target_tau_powers_length points)
    println!("Copying tau_powers_g2...");
    stream_copy_points::<G2Affine, _>(&mut input, &mut output, target_tau_powers_length, Bls12CeremonyParameters::G2_UNCOMPRESSED_BYTE_SIZE)?;
    
    // Skip remaining tau_powers_g2 points
    let skip_g2_points = current_tau_powers_length - target_tau_powers_length;
    if skip_g2_points > 0 {
        skip_points(&mut input, skip_g2_points, Bls12CeremonyParameters::G2_UNCOMPRESSED_BYTE_SIZE)?;
    }

    // Stream copy alpha_tau_powers_g1 (first target_tau_powers_length points)
    println!("Copying alpha_tau_powers_g1...");
    stream_copy_points::<G1Affine, _>(&mut input, &mut output, target_tau_powers_length, Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE)?;
    
    // Skip remaining alpha_tau_powers_g1 points
    let skip_g1_points = current_tau_powers_length - target_tau_powers_length;
    if skip_g1_points > 0 {
        skip_points(&mut input, skip_g1_points, Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE)?;
    }

    // Stream copy beta_tau_powers_g1 (first target_tau_powers_length points)
    println!("Copying beta_tau_powers_g1...");
    stream_copy_points::<G1Affine, _>(&mut input, &mut output, target_tau_powers_length, Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE)?;
    
    // Skip remaining beta_tau_powers_g1 points
    if skip_g1_points > 0 {
        skip_points(&mut input, skip_g1_points, Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE)?;
    }

    // Copy beta_g2 (always just 1 point)
    println!("Copying beta_g2...");
    stream_copy_points::<G2Affine, _>(&mut input, &mut output, 1, Bls12CeremonyParameters::G2_UNCOMPRESSED_BYTE_SIZE)?;

    output.flush().expect("unable to flush output file");

    println!("Successfully wrote reduced challenge to '{}'", output_file);

    verify_reduced_challenge(output_file, target_power)?;

    Ok(())
}

fn detect_power_from_size(accumulator_size: usize, _parameters: &Bls12CeremonyParameters) -> usize {
    // Try different powers to find which one matches the file size
    for power in 10..=27 {
        let tau_powers_length = 1 << power;
        let tau_powers_g1_length = (tau_powers_length << 1) - 1;
        
        let expected_size = (tau_powers_g1_length * Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE) +
                           (tau_powers_length * Bls12CeremonyParameters::G2_UNCOMPRESSED_BYTE_SIZE) +
                           (tau_powers_length * Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE) +
                           (tau_powers_length * Bls12CeremonyParameters::G1_UNCOMPRESSED_BYTE_SIZE) +
                           Bls12CeremonyParameters::G2_UNCOMPRESSED_BYTE_SIZE;
        
        if expected_size == accumulator_size {
            return power;
        }
    }
    
    panic!("Could not detect power from file size {} bytes", accumulator_size);
}

fn stream_copy_points<G: CurveAffine, W: Write>(
    input: &mut dyn Read,
    output: &mut W,
    count: usize,
    point_size: usize
) -> std::io::Result<()> {
    let mut buffer = vec![0u8; point_size];
    
    for _ in 0..count {
        input.read_exact(&mut buffer)?;
        output.write_all(&buffer)?;
    }
    
    Ok(())
}

fn skip_points(input: &mut dyn Read, count: usize, point_size: usize) -> std::io::Result<()> {
    let mut buffer = vec![0u8; point_size];
    
    for _ in 0..count {
        input.read_exact(&mut buffer)?;
    }
    
    Ok(())
}

fn verify_reduced_challenge(output_file: &str, target_power: usize) -> Result<(), Box<dyn std::error::Error>> {
    println!("Verifying reduced challenge file...");
    
    // Open the output file
    let file = OpenOptions::new()
        .read(true)
        .open(output_file)
        .expect(&format!("unable to open output file '{}'", output_file));
    
    let mut reader = BufReader::new(file);
    
    // Skip the 64-byte hash
    let mut hash = [0u8; 64];
    reader.read_exact(&mut hash)?;
    
    // Calculate expected sizes
    let expected_tau_powers_length = 1 << target_power;
    let expected_tau_powers_g1_length = (expected_tau_powers_length << 1) - 1;
    
    // Manually verify the accumulator structure by reading and checking each section
    println!("Reading and verifying tau_powers_g1 ({} points)...", expected_tau_powers_g1_length);
    verify_g1_points(&mut reader, expected_tau_powers_g1_length)?;
    
    println!("Reading and verifying tau_powers_g2 ({} points)...", expected_tau_powers_length);
    verify_g2_points(&mut reader, expected_tau_powers_length)?;
    
    println!("Reading and verifying alpha_tau_powers_g1 ({} points)...", expected_tau_powers_length);
    verify_g1_points(&mut reader, expected_tau_powers_length)?;
    
    println!("Reading and verifying beta_tau_powers_g1 ({} points)...", expected_tau_powers_length);
    verify_g1_points(&mut reader, expected_tau_powers_length)?;
    
    println!("Reading and verifying beta_g2 (1 point)...");
    verify_g2_points(&mut reader, 1)?;
    
    println!("✓ Successfully verified reduced challenge file");
    println!("  - All curve points are valid");
    println!("  - No points at infinity found");
    println!("  - Accumulator structure is correct for 2^{}", target_power);
    
    Ok(())
}

fn verify_g1_points<R: Read>(reader: &mut R, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    use bellman::pairing::EncodedPoint;
    use bellman::pairing::bls12_381::G1Uncompressed;
    
    for i in 0..count {
        let mut encoded = G1Uncompressed::empty();
        reader.read_exact(encoded.as_mut())?;
        
        match encoded.into_affine() {
            Ok(point) => {
                if point.is_zero() {
                    return Err(format!("Point at infinity found at G1 index {}", i).into());
                }
            },
            Err(e) => {
                return Err(format!("Invalid G1 point at index {}: {:?}", i, e).into());
            }
        }
    }
    
    Ok(())
}

fn verify_g2_points<R: Read>(reader: &mut R, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    use bellman::pairing::EncodedPoint;
    use bellman::pairing::bls12_381::G2Uncompressed;
    
    for i in 0..count {
        let mut encoded = G2Uncompressed::empty();
        reader.read_exact(encoded.as_mut())?;
        
        match encoded.into_affine() {
            Ok(point) => {
                if point.is_zero() {
                    return Err(format!("Point at infinity found at G2 index {}", i).into());
                }
            },
            Err(e) => {
                return Err(format!("Invalid G2 point at index {}: {:?}", i, e).into());
            }
        }
    }
    
    Ok(())
}
