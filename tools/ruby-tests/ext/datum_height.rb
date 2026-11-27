# Written like Add Height from Datum: a datum is kept as an attribute on the model; a tool measures where you
# click against it and drops a text label with the height. Uses model attributes, Sketchup.read_default,
# input points, add_text and UI.inputbox.
module SCFixtures
  class HeightTool
    def activate
      @datum = Sketchup.active_model.get_attribute('SCHeight', 'datum', 0.0)
      Sketchup.status_text = "Click to label a height above the datum (#{@datum.to_l} in)"
    end

    def onMouseMove(flags, x, y, view)
      view.tooltip = 'height'
    end

    def onLButtonDown(flags, x, y, view)
      ip = view.inputpoint(x, y)
      return unless ip.valid?
      h = ip.position.z - @datum
      model = view.model
      model.start_operation('Add height', true)
      t = model.active_entities.add_text("#{(h * 0.0254).round(2)} m", ip.position, Geom::Vector3d.new(0, 0, 12))
      t.set_attribute('SCHeight', 'point_z', ip.position.z)
      model.commit_operation
    end
  end
end

UI.menu('Plugins').add_item('Add height above datum') do
  Sketchup.active_model.select_tool(SCFixtures::HeightTool.new)
end

UI.menu('Plugins').add_item('Set datum') do
  res = UI.inputbox(['Datum (z)'], [Sketchup.read_default('SCHeight', 'datum', 0.0)], 'Set datum')
  next unless res
  Sketchup.active_model.set_attribute('SCHeight', 'datum', res[0])
  Sketchup.write_default('SCHeight', 'datum', res[0])
  puts "datum #{res[0]}"
end
