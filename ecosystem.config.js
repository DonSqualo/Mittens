module.exports = {
  apps: [
    {
      name: 'mittens-server',
      cwd: '/home/heim/clawd/Mittens',
      script: './server/target/release/scriptcad-server',
      args: 'examples/multiphysics/hallbach.lua',
      env: {
        LD_LIBRARY_PATH: '/home/heim/clawd/Mittens/server/target/release/build/manifold3d-sys-c41f9fc9d0e23d46/out/lib'
      },
      watch: false,
      autorestart: true,
    },
    {
      name: 'mittens-renderer',
      cwd: '/home/heim/clawd/Mittens/renderer',
      script: './node_modules/.bin/vite',
      args: '--port 3000 --host 0.0.0.0',
      watch: false,
      autorestart: true,
    }
  ]
};
