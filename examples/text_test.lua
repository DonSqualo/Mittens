-- Test the text() primitive
-- Updated to reload

local Mittens = require("stdlib")

-- Create a text label with font size 12
local label = text("HELLO", 12)
    :at(0, 0, 0)
    :name("text_label")

-- Also create a sphere to show we can combine it with other primitives
local sphere = sphere(3)
    :at(10, 0, 5)
    :name("sphere")

-- Combine them with a union
local scene = union(label, sphere)

Mittens.register(scene)

return Mittens.serialize()
