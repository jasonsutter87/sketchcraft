# Observers, a dialog with callbacks, a timer and a texture: what extensions with a panel do.
module SCFixtures
  COUNT = { selection: 0, added: 0, commits: 0, ticks: 0 }

  class SelObserver < Sketchup::SelectionObserver
    def onSelectionBulkChange(selection); COUNT[:selection] += 1; end
    def onSelectionCleared(selection); COUNT[:selection] += 100; end
  end

  class EntObserver < Sketchup::EntitiesObserver
    def onElementAdded(entities, entity); COUNT[:added] += 1; end
  end

  class ModObserver < Sketchup::ModelObserver
    def onTransactionCommit(model); COUNT[:commits] += 1; end
  end
end

UI.menu('Plugins').add_item('Panel') do
  model = Sketchup.active_model
  model.selection.add_observer(SCFixtures::SelObserver.new)
  model.entities.add_observer(SCFixtures::EntObserver.new)
  model.add_observer(SCFixtures::ModObserver.new)

  dlg = UI::HtmlDialog.new(dialog_title: 'Panel', width: 300, height: 200)
  dlg.set_html('<button onclick="sketchup.make_box(\'2\')">Make box</button>')
  dlg.add_action_callback('make_box') do |_ctx, size|
    model.start_operation('Box', true)
    s = size.to_f.m
    f = model.active_entities.add_face([0, 0, 0], [s, 0, 0], [s, s, 0], [0, s, 0])
    f.pushpull(-s)
    model.commit_operation
    dlg.execute_script("document.title = 'made #{SCFixtures::COUNT[:added]}'")
  end
  dlg.add_action_callback('report') { |_c, _p| puts SCFixtures::COUNT.map { |k, v| "#{k}=#{v}" }.join(' ') }
  dlg.show
  UI.start_timer(0.5, false) { SCFixtures::COUNT[:ticks] += 1 }
end

UI.menu('Plugins').add_item('Textured') do
  m = Sketchup.active_model.materials.add('Brick')
  m.texture = ['data:image/png;base64,iVBORw0KGgo=', 8.0, 4.0]
  f = Sketchup.active_model.entities.add_face([0, 0, 0], [40, 0, 0], [40, 0, 40], [0, 0, 40])
  f.material = m
  puts "texture #{m.texture.width}x#{m.texture.height}"
end
