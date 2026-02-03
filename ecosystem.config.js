module.exports = {
  apps: [
    {
      name: 'mittens-server',
      cwd: './server',
      script: './target/release/scriptcad-server',
      args: '../project/extrusion_test.lua',
      interpreter: 'none',
      watch: false,
      env: {
        RUST_LOG: 'info',
        LD_LIBRARY_PATH: '/home/heim/clawd/Mittens/server/target/release/build/manifold3d-sys-c41f9fc9d0e23d46/out/lib'
      }
    },
    {
      name: 'mittens-renderer',
      cwd: './renderer',
      script: 'npx',
      args: 'vite --host 0.0.0.0',
      interpreter: 'none',
      watch: false
    }
  ]
};
