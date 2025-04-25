# noodles_grid

**noodles_grid** is a real-time visualization server for power system simulation data.  
It loads precomputed datasets, builds optimized instance renderings, and streams live interactive scenes to connected clients.

Built for high scalability, flexible visualization, and fast interaction.

---

## Features

- 📈 **Visualize power system data** — voltages, real power, reactive power, line loads
- 🔎 **Interactive probes** — drag and drop probes to inspect nearby line data
- 🛠 **Instanced rendering** — buses, lines, transformers, generators, hazards, and flows
- 🗺 **Floorplan support** — map datasets over embedded floorplan images
- 🔄 **Automatic timestep playback** — play, pause, or step through simulation time
- 🧩 **Extensible server API** — built-in method invocation system
- 🌐 **Zero-config discovery** — mDNS advertising for easy local client discovery

---

## How It Works

1. **Load Dataset**: 
   - A Cap'n Proto file (`*.bin`) describing the system lines, transformers, and generators.
2. **Setup Geometry**:
   - Instanced geometry is created for every object type, optimized for GPU uploads.
3. **Stream Scene**:
   - A lightweight server pushes updates to clients using [colabrodo](https://github.com/InsightCenterNoodles/colabrodo) infrastructure.
4. **Client Interaction**:
   - Users can step through time, move probes, inspect data, and trigger events.

---

## Quickstart

### Requirements
- Rust 1.74+
- Cap'n Proto installed (for dataset generation)
- A `.bin` dataset file to visualize

### Build and Run

```bash
git clone https://github.com/nicholasbl/noodle_grid.git
cd noodles_grid
cargo run --release -- your_dataset.pack
```

The server will print:

```
Connect clients to port: 50000
```

Clients can connect directly, or discover via Bonjour/mDNS.

---

## Arguments

| Argument      | Description                 | Default      |
| ------------- | --------------------------- | ------------ |
| `--port`      | Port to host server on      | `50000`      |
| `--pack-path` | Path to `.bin` dataset file | *(Required)* |

---

## Interaction

- **Base Map**:  
  An optional floorplan or satellite image is displayed flat on the ground for geographic context.

- **Conductors**:
  - Lines are shown rising up from the base map.
  - Line height is determined by *unit voltage* (higher voltage floats higher).
  - Line thickness is proportional to real and reactive power flow.

- **Buses**:
  - Shown as small points between connected conductors.

- **Transformers**:
  - Represented as vertical lines plunging from elevated conductors into the ground.

- **Generators**:
  - Shown as distinctive glyphs, sized according to real power output.

- **View Mode Switching**:
  - Using the **"Toggle Line Load"** method, users can switch to a *percentage load view*, where line height reflects how much of their rated capacity the lines are using.

- **Probes**:
  - Users can use the **"Create Probe"** method to place movable probes on the scene.
  - Probes attach to the nearest conductor and automatically generate live charts of voltage, real power, and reactive power over time.

- **Bird's Eye View**:
  - Scene defaults to a top-down view for easy understanding of grid layout.

---

## Development

- Project is modularized into:
  - `state` — core server state machine
  - `instance` — dynamic instance buffer generation
  - `methods` — API methods for interaction
  - `geometry` — primitive generation
  - `texture` — embedded texture management
  - `probe` — interactive probe system
  - `chart` — chart generation (plotters backend)
  - `ruler` — floor ruler generation
  - `import_obj` — OBJ loader utilities

- Everything is documented for `cargo doc` output.

To generate developer documentation:

```bash
cargo doc --open
```

---

## License

[MIT License](LICENSE)

---

## Screenshots

* To be added!