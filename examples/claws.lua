local Mittens = require("stdlib")

Arm = {
  min_width = 10,
  max_width = 15,
  length = 120,
  height = 10,
  min_height = 2,
}
Arm.rotation = 3
Keyboard = {
  length = 150
}
Hinge = {
  M3 = 3,
  slider = {
    w = 2,
    h = 3
  },
  wall = 1,
  width = Arm.max_width
}
Hinge.height = Hinge.slider.h + 2 * Hinge.wall
Hinge.length = Hinge.slider.w + 4 * Hinge.wall + Hinge.M3

Arm.body = difference(
  box(Arm.length, Arm.max_width, Arm.height),
  box(Arm.length * 2, Arm.max_width, Arm.height):at(0, 0, Arm.min_height):rotate(0, Arm.rotation, 0)
):at(-Arm.length, 0, 0)

Hinge.body = difference(
  box(Hinge.length, Hinge.width, Hinge.height),
  box(Hinge.slider.w, Hinge.width, Hinge.slider.h):at(Hinge.wall, 0, Hinge.wall)
)


Hinge.body = difference(
  union(Hinge.body, cylinder(Hinge.M2 + Hinge.wall, Hinge.width):rotate(89, 0, 0)),
  cylinder(Hinge.M2 / 2, Hinge.width)
)

Assembly = group("Assemby", {
  Hinge.body:color(0.3, 0.62, 0.62),
  -- Arm.body
})


Mittens.register(Assembly)

return Mittens.serialize()
