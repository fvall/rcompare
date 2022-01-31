doc:
  rustup doc std
  cargo doc --open
cache:
  sudo sh -c "echo 1 > /proc/sys/vm/drop_caches"
