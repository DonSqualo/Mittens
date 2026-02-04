module.exports = {
  apps: [
    {
      name: 'victor-mittens-server',
      cwd: './server',
      script: './target/release/scriptcad-server',
      args: '../projects/helmholtz/helmholtz_coil.lua',
      interpreter: 'none',
      watch: false,
      env: {
        RUST_LOG: 'info',
        MITTENS_PORT: '3003',
        LD_LIBRARY_PATH: '/home/heim/clawd-victor/Private_Mittens/server/target/release/build/manifold3d-sys-c41f9fc9d0e23d46/out/lib'
      }
    },
    {
      name: 'victor-mittens-renderer',
      cwd: './renderer',
      script: 'npx',
      args: 'vite --host 0.0.0.0 --port 3002',
      interpreter: 'none',
      watch: false
    }
  ]
};

