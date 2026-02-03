-- Mittens View State
-- This file is auto-saved to persist camera position and visibility settings
-- You can edit it manually or it will be updated when you change the view

return {
  camera = {
    position = {150, 100, 80},
    target = {0, 0, 0},
    up = {0, 0, 1},
    fov = 45,
    projection = "perspective"
  },

  visible = {
    "lymph_bath",
    "bath_shell",
    "bath_water",
    "gel_matrix",
    "lymphatic_network",
    "channel_trunk",
    "speaker_left",
    "speaker_right",
    -- "debug_objects",  -- Commented items are hidden
  },

  hidden = {
    -- Items explicitly hidden
  },

  clip = nil,  -- Set to { plane = "XZ", offset = 0 } to enable clipping

  transparency = {
    -- Override transparency per object
    -- substrate = 0.3,
  },

  instruments = {
    show_probes = true,
    show_field_planes = true,
    show_streamlines = false,
  },

  theme = "dark",

  grid = {
    show = true,
    size = 100,
    divisions = 10,
    color = {0.2, 0.2, 0.25}
  },

  axes = {
    show = true,
    size = 20
  },

  render = {
    quality = "high",
    shadows = true,
    ambient_occlusion = true,
    anti_aliasing = true,
    max_ray_steps = 128
  }
}
