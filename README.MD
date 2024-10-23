# LoRaWAN wb25模组用户端应用程序

### 生成bin
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabi/release/lrw-app lrw-app.bin

### 编译不带defmt
cargo run --release --no-default-features