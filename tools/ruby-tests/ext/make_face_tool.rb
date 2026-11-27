# Written like s4u Make Face: look at the faces and edges of what is in the model, and add a face where a
# closed loop of edges has none. Here it also reads the model (the faces that are there), uses Follow Me to
# sweep a small profile along the outline, and puts the result in a group.
UI.menu('Plugins').add_item('Make face from outline') do
  model = Sketchup.active_model
  ents = model.active_entities
  faces = ents.grep(Sketchup::Face)
  next UI.messagebox('Nothing to work from: the model has no face') if faces.empty?
  base = faces.max_by(&:area)
  model.start_operation('Make Face', true)
  # the outline of the biggest face, grown by 0.5 m
  pts = base.outer_loop.vertices.map(&:position)
  puts "outline of #{pts.length} points, area #{base.area.round}"
  g = ents.add_group
  face = g.entities.add_face(pts.map { |p| p + Geom::Vector3d.new(0, 0, 40) })
  face.pushpull(-20)
  model.commit_operation
end

UI.menu('Plugins').add_item('Rail around outline') do
  model = Sketchup.active_model
  ents = model.active_entities
  base = ents.grep(Sketchup::Face).first
  pts = base.vertices.map(&:position)
  path = ents.add_edges(pts + [pts[0]])
  # a 2 in square profile standing at the first corner
  c = pts[0]
  profile = ents.add_face([c, c + Geom::Vector3d.new(0, 0, 2), c + Geom::Vector3d.new(0, 2, 2), c + Geom::Vector3d.new(0, 2, 0)])
  profile.followme(path)
end
