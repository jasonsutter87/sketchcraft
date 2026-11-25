# Uses an API SketchCraft doesn't have (importing a file into the model): should say so clearly.
UI.menu('Plugins').add_item('Import a file') do
  Sketchup.active_model.import('/tmp/x.dae')
end
