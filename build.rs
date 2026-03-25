fn main() {
    #[cfg(feature = "audio")]
    audio_model::download_if_needed();

    // Always set the env var so `env!()` compiles even without the audio feature.
    #[cfg(not(feature = "audio"))]
    println!("cargo:rustc-env=WHISPER_MODEL_PATH_DEFAULT=");
}

#[cfg(feature = "audio")]
mod audio_model {
    use std::fs;
    use std::path::PathBuf;

    const MODEL_NAME: &str = "ggml-base.en.bin";
    const MODEL_URL: &str =
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

    /// Platform cache directory: `$HOME/.cache/whisper/`
    fn cache_dir() -> PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".cache").join("whisper")
        } else {
            PathBuf::from("whisper-models")
        }
    }

    pub fn download_if_needed() {
        let dir = cache_dir();
        let model_path = dir.join(MODEL_NAME);

        // Expose the model path to the compiled crate via env var
        println!(
            "cargo:rustc-env=WHISPER_MODEL_PATH_DEFAULT={}",
            model_path.display()
        );

        // Already downloaded — nothing to do
        if model_path.exists() {
            println!(
                "cargo:warning=Whisper model already cached at {}",
                model_path.display()
            );
            return;
        }

        println!(
            "cargo:warning=Downloading whisper model to {} ...",
            model_path.display()
        );

        if let Err(e) = do_download(&dir, &model_path) {
            println!("cargo:warning=Failed to download whisper model: {e}");
            println!("cargo:warning=Set WHISPER_MODEL_PATH manually before running.");
        }
    }

    fn do_download(
        dir: &std::path::Path,
        dest: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(dir)?;

        let tmp = dest.with_extension("bin.part");

        let status = std::process::Command::new("curl")
            .args(["-fSL", "-o"])
            .arg(&tmp)
            .arg(MODEL_URL)
            .status()?;

        if !status.success() {
            return Err(format!("curl exited with status {status}").into());
        }

        fs::rename(&tmp, dest)?;

        println!(
            "cargo:warning=Whisper model downloaded to {}",
            dest.display()
        );

        Ok(())
    }
}
