#!/bin/bash
export LD_LIBRARY_PATH="/home/heim/clawd/Private_Mittens/server/target/release/build/manifold3d-sys-c41f9fc9d0e23d46/out/lib:$LD_LIBRARY_PATH"
exec /home/heim/clawd/Private_Mittens/server/target/release/mittens-server "$@"
