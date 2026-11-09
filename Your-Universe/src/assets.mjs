// Localtime asset loader (fragment "assets").
//
// Reads assets/manifest.json, loads the pack GLBs it lists and returns ready to
// instance models. The manifest and pack contract is fixed in src/CONTRACT.md:
// one GLB per pack, one shared palette material per pack, top-level nodes named
// after the manifest model. Every model is baked into one geometry whose origin
// is the base centre (min y = 0, x/z centred) so world.mjs can place it with a
// plain matrix.
//
// The loader never throws: load() resolves with Assets or null (one console.warn)
// so main can keep the procedural fallback. It also never blocks the first paint:
// call it after the scene exists, then swap meshes in.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

export function createAssetLoader(THREE, GLTFLoader) {
  var DEFAULT_MANIFEST_URL = 'assets/manifest.json';
  var DEFAULT_TIMEOUT_MS = 8000;
  var warned = false;

  function warnOnce(message) {
    if (warned) return;
    warned = true;
    if (typeof console !== 'undefined' && console.warn) console.warn('[assets] ' + message);
  }

  function pageBase() {
    if (typeof document !== 'undefined' && document && document.baseURI) return document.baseURI;
    if (typeof location !== 'undefined' && location && location.href) return location.href;
    return null;
  }

  // Manifest paths are written for the site root (assets/kenney-car-kit.glb), so they
  // resolve against the document base, not against the manifest URL.
  function resolveUrl(path) {
    var base = pageBase();
    if (!base) return path;
    try {
      return new URL(path, base).href;
    } catch (error) {
      return path;
    }
  }

  function tierOf(spec) {
    return spec && spec.tier === 'street' ? 'street' : 'district';
  }

  function reportProgress(context, info) {
    if (!context.onProgress) return;
    try {
      context.onProgress(info);
    } catch (error) {
      // a broken progress callback must never fail the load
    }
  }

  function fetchJson(target, context) {
    return fetch(target, {
      signal: context.controller ? context.controller.signal : undefined,
      credentials: 'same-origin'
    }).then(function (response) {
      if (!response.ok) throw new Error('HTTP ' + response.status + ' for ' + target);
      return response.json();
    });
  }

  function fetchPack(target, context, packId, packIndex, packTotal) {
    return fetch(target, {
      signal: context.controller ? context.controller.signal : undefined,
      credentials: 'same-origin'
    }).then(function (response) {
      if (!response.ok) throw new Error('HTTP ' + response.status + ' for ' + target);
      var declared = Number(response.headers.get('content-length')) || 0;
      reportProgress(context, {
        phase: 'pack',
        pack: packId,
        packIndex: packIndex,
        packs: packTotal,
        loaded: 0,
        total: declared
      });
      var reader = response.body && typeof response.body.getReader === 'function' ? response.body.getReader() : null;
      if (!reader) {
        return response.arrayBuffer().then(function (whole) {
          reportProgress(context, {
            phase: 'pack',
            pack: packId,
            packIndex: packIndex,
            packs: packTotal,
            loaded: whole.byteLength,
            total: whole.byteLength
          });
          return whole;
        });
      }
      var chunks = [];
      var loaded = 0;
      function pump() {
        return reader.read().then(function (step) {
          if (step.done) {
            var bytes = new Uint8Array(loaded);
            var offset = 0;
            for (var i = 0; i < chunks.length; i += 1) {
              bytes.set(chunks[i], offset);
              offset += chunks[i].length;
            }
            return bytes.buffer;
          }
          var value = step.value;
          if (value && value.length) {
            chunks.push(value);
            loaded += value.length;
            reportProgress(context, {
              phase: 'pack',
              pack: packId,
              packIndex: packIndex,
              packs: packTotal,
              loaded: loaded,
              total: declared
            });
          }
          return pump();
        });
      }
      return pump();
    });
  }

  function validateManifest(manifest) {
    if (!manifest || typeof manifest !== 'object') throw new Error('manifest is not an object');
    if (manifest.version !== 1) throw new Error('unsupported manifest version ' + String(manifest.version));
    if (!Array.isArray(manifest.packs)) throw new Error('manifest has no packs array');
    if (!manifest.models || typeof manifest.models !== 'object') throw new Error('manifest has no models object');
    var packsById = new Map();
    for (var i = 0; i < manifest.packs.length; i += 1) {
      var pack = manifest.packs[i];
      if (!pack || typeof pack.id !== 'string' || !pack.id) throw new Error('manifest pack ' + i + ' has no id');
      if (typeof pack.file !== 'string' || !pack.file) throw new Error('manifest pack ' + pack.id + ' has no file');
      if (packsById.has(pack.id)) throw new Error('duplicate manifest pack id ' + pack.id);
      packsById.set(pack.id, pack);
    }
    return packsById;
  }

  function collectModels(manifest, tier, packsById) {
    var names = Object.keys(manifest.models);
    var wanted = [];
    for (var i = 0; i < names.length; i += 1) {
      var name = names[i];
      var spec = manifest.models[name];
      if (!spec || typeof spec !== 'object') continue;
      if (tierOf(spec) !== tier) continue;
      if (typeof spec.pack !== 'string' || !spec.pack) {
        throw new Error('manifest model "' + name + '" has no pack');
      }
      if (!packsById.has(spec.pack)) {
        throw new Error('manifest model "' + name + '" points at unknown pack "' + spec.pack + '"');
      }
      wanted.push({ name: name, spec: spec });
    }
    return wanted;
  }

  function groupByPack(wanted, packsById) {
    var groups = [];
    var indexById = new Map();
    for (var i = 0; i < wanted.length; i += 1) {
      var item = wanted[i];
      var packId = item.spec.pack;
      if (!indexById.has(packId)) {
        indexById.set(packId, groups.length);
        groups.push({ pack: packsById.get(packId), items: [] });
      }
      groups[indexById.get(packId)].items.push(item);
    }
    return groups;
  }

  function findNode(scene, name) {
    for (var i = 0; i < scene.children.length; i += 1) {
      if (scene.children[i].name === name) return scene.children[i];
    }
    var found = null;
    scene.traverse(function (object) {
      if (found || object === scene) return;
      if (object.name === name) found = object;
    });
    return found;
  }

  // COLOR_0 arrives as a normalised u8 or u16 attribute on the nature pack. Three exposes the
  // raw integers in the backing array, so the scale is applied here instead of trusting the
  // attribute's normalised flag: the baked copy is always a float triple in 0..1.
  function readVertexColor(attribute, at, out) {
    var array = attribute.array;
    var width = attribute.itemSize;
    var base = at * width;
    var scale = 1;
    if (attribute.normalized) {
      if (array.BYTES_PER_ELEMENT === 1) scale = 1 / 255;
      else if (array.BYTES_PER_ELEMENT === 2) scale = 1 / 65535;
    }
    out[0] = array[base] * scale;
    out[1] = array[base + 1] * scale;
    out[2] = array[base + 2] * scale;
  }

  function bakeNode(node, name, wantVertexColors) {
    node.updateWorldMatrix(true, true);
    var positions = [];
    var normals = [];
    var uvs = [];
    var colors = wantVertexColors ? [] : null;
    var colorScratch = wantVertexColors ? [1, 1, 1] : null;
    var normalsComplete = true;
    var triangleCount = 0;
    var sourceMaterial = null;
    var vertex = new THREE.Vector3();
    var normal = new THREE.Vector3();
    var normalMatrix = new THREE.Matrix3();

    node.traverse(function (child) {
      if (!child.isMesh || !child.geometry || child.isSkinnedMesh) return;
      var geometry = child.geometry;
      var position = geometry.getAttribute('position');
      if (!position) return;
      var normalAttr = geometry.getAttribute('normal') || null;
      var uvAttr = geometry.getAttribute('uv') || geometry.getAttribute('uv1') || null;
      var colorAttr = colors ? (geometry.getAttribute('color') || null) : null;
      var index = geometry.getIndex ? geometry.getIndex() : null;
      var count = index ? index.count : position.count;
      if (count < 3) return;

      if (!sourceMaterial) {
        var candidate = Array.isArray(child.material) ? child.material[0] : child.material;
        if (candidate) sourceMaterial = candidate;
      }
      normalMatrix.getNormalMatrix(child.matrixWorld);

      for (var v = 0; v < count; v += 1) {
        var at = index ? index.getX(v) : v;
        vertex.fromBufferAttribute(position, at).applyMatrix4(child.matrixWorld);
        positions.push(vertex.x, vertex.y, vertex.z);
        if (normalAttr) {
          normal.fromBufferAttribute(normalAttr, at).applyMatrix3(normalMatrix);
          if (normal.lengthSq() > 0) normal.normalize();
          normals.push(normal.x, normal.y, normal.z);
        } else {
          normalsComplete = false;
          normals.push(0, 0, 0);
        }
        if (uvAttr) uvs.push(uvAttr.getX(at), uvAttr.getY(at));
        else uvs.push(0, 0);
        if (colors) {
          if (colorAttr && colorAttr.itemSize >= 3) readVertexColor(colorAttr, at, colorScratch);
          else {
            colorScratch[0] = 1;
            colorScratch[1] = 1;
            colorScratch[2] = 1;
          }
          colors.push(colorScratch[0], colorScratch[1], colorScratch[2]);
        }
      }
      triangleCount += count / 3;
    });

    if (positions.length === 0) throw new Error('model "' + name + '" has no mesh vertices');

    if (!normalsComplete) {
      var temporary = new THREE.BufferGeometry();
      temporary.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
      temporary.computeVertexNormals();
      var computed = temporary.getAttribute('normal');
      normals.length = 0;
      for (var c = 0; c < computed.count; c += 1) {
        normals.push(computed.getX(c), computed.getY(c), computed.getZ(c));
      }
      temporary.dispose();
    }

    var minX = Infinity;
    var minY = Infinity;
    var minZ = Infinity;
    var maxX = -Infinity;
    var maxY = -Infinity;
    var maxZ = -Infinity;
    for (var p = 0; p < positions.length; p += 3) {
      var x = positions[p];
      var y = positions[p + 1];
      var z = positions[p + 2];
      if (x < minX) minX = x;
      if (y < minY) minY = y;
      if (z < minZ) minZ = z;
      if (x > maxX) maxX = x;
      if (y > maxY) maxY = y;
      if (z > maxZ) maxZ = z;
    }
    var centerX = (minX + maxX) * 0.5;
    var centerZ = (minZ + maxZ) * 0.5;
    for (var q = 0; q < positions.length; q += 3) {
      positions[q] -= centerX;
      positions[q + 1] -= minY;
      positions[q + 2] -= centerZ;
    }

    var baked = new THREE.BufferGeometry();
    baked.name = name;
    baked.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    baked.setAttribute('normal', new THREE.Float32BufferAttribute(normals, 3));
    baked.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
    if (colors) baked.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
    baked.computeBoundingBox();
    baked.computeBoundingSphere();
    var bounds = baked.boundingBox.clone();
    var size = bounds.getSize(new THREE.Vector3());

    return {
      geometry: baked,
      bounds: bounds,
      footprint: { w: size.x, d: size.z, h: size.y },
      tris: Math.round(triangleCount),
      sourceMaterial: sourceMaterial
    };
  }

  // One material per pack: the palette texture is shared by every primitive and
  // every model of the pack, so instancing each model costs one draw call each.
  function createPackMaterial(packId, source, pack) {
    var material = new THREE.MeshStandardMaterial();
    // A pack whose colours live in COLOR_0 (the nature kit) samples no palette texture, so the
    // baked vertex colours are its only albedo. One material per pack still covers every model
    // of that pack; the flag comes from the manifest, not from guessing at the geometry.
    if (pack && pack.vertexColors === true) material.vertexColors = true;
    if (source) {
      if (source.map) material.map = source.map;
      if (typeof source.roughness === 'number') material.roughness = source.roughness;
      if (typeof source.metalness === 'number') material.metalness = source.metalness;
      if (typeof source.alphaTest === 'number') material.alphaTest = source.alphaTest;
      if (typeof source.opacity === 'number') material.opacity = source.opacity;
      if (source.transparent === true) material.transparent = true;
      if (source.side !== undefined) material.side = source.side;
    }
    material.color.setRGB(1, 1, 1);
    material.name = 'pack:' + packId;
    if (material.map) material.map.colorSpace = THREE.SRGBColorSpace;
    return material;
  }

  function buildPackModels(gltf, pack, items) {
    var scene = gltf.scene || (gltf.scenes && gltf.scenes[0]);
    if (!scene) throw new Error('pack ' + pack.id + ' has no scene');
    scene.updateMatrixWorld(true);

    var bakedItems = [];
    var sourceMaterial = null;
    var wantVertexColors = pack.vertexColors === true;
    for (var i = 0; i < items.length; i += 1) {
      var item = items[i];
      var node = findNode(scene, item.name);
      if (!node) throw new Error('pack ' + pack.id + ' has no node named "' + item.name + '"');
      var baked = bakeNode(node, item.name, wantVertexColors);
      if (!sourceMaterial && baked.sourceMaterial) sourceMaterial = baked.sourceMaterial;
      bakedItems.push({ item: item, baked: baked });
    }

    var material = createPackMaterial(pack.id, sourceMaterial, pack);
    var models = [];
    for (var k = 0; k < bakedItems.length; k += 1) {
      var entry = bakedItems[k];
      models.push({
        name: entry.item.name,
        category:
          typeof entry.item.spec.category === 'string' && entry.item.spec.category
            ? entry.item.spec.category
            : 'prop',
        tier: tierOf(entry.item.spec),
        geometry: entry.baked.geometry,
        material: material,
        bounds: entry.baked.bounds,
        footprint: entry.baked.footprint,
        tris: entry.baked.tris,
        tintable: entry.item.spec.tintable === true
      });
    }

    // The merged geometry owns copied vertex data; the source scene is no longer needed.
    scene.traverse(function (child) {
      if (child.isMesh && child.geometry && typeof child.geometry.dispose === 'function') {
        child.geometry.dispose();
      }
    });
    return models;
  }

  function parsePack(buffer, packUrl) {
    var loader = new GLTFLoader();
    var slash = packUrl.lastIndexOf('/');
    var basePath = slash >= 0 ? packUrl.slice(0, slash + 1) : '';
    return loader.parseAsync(buffer, basePath);
  }

  function runLoad(url, context) {
    if (typeof GLTFLoader !== 'function') throw new Error('GLTFLoader constructor is missing');
    var manifestUrl = resolveUrl(url);
    return fetchJson(manifestUrl, context).then(function (manifest) {
      var packsById = validateManifest(manifest);
      var wanted = collectModels(manifest, context.tier, packsById);
      if (wanted.length === 0) throw new Error('manifest lists no ' + context.tier + ' models');
      var groups = groupByPack(wanted, packsById);
      context.packCount = groups.length;
      reportProgress(context, { phase: 'manifest', models: wanted.length, packs: groups.length });
      return groups.reduce(function (chain, group, groupIndex) {
        return chain.then(function (models) {
          var packUrl = resolveUrl(group.pack.file);
          return fetchPack(packUrl, context, group.pack.id, groupIndex, groups.length)
            .then(function (buffer) {
              return parsePack(buffer, packUrl);
            })
            .then(function (gltf) {
              var built = buildPackModels(gltf, group.pack, group.items);
              for (var i = 0; i < built.length; i += 1) models.set(built[i].name, built[i]);
              return models;
            });
        });
      }, Promise.resolve(new Map()));
    }).then(function (models) {
      if (models.size === 0) throw new Error('no models were built');

      // The category index is recomputed from the live Map on every call instead of from a
      // snapshot of names: main merges the street tier into the same Map after the district
      // load, and a captured name list would keep every street model invisible to byCategory
      // and pick even though it is present in models.
      function byCategory(category) {
        var out = [];
        models.forEach(function (model) {
          if (model && model.category === category) out.push(model);
        });
        return out;
      }

      function hashSeed(seed) {
        if (typeof seed === 'number' && isFinite(seed)) return Math.abs(Math.floor(seed));
        var text = String(seed === undefined || seed === null ? 0 : seed);
        var hash = 2166136261;
        for (var i = 0; i < text.length; i += 1) {
          hash ^= text.charCodeAt(i);
          hash = Math.imul(hash, 16777619);
        }
        return hash >>> 0;
      }

      function pick(category, seed) {
        var list = byCategory(category);
        if (list.length === 0) return null;
        return list[hashSeed(seed) % list.length];
      }

      function makeInstanced(model, count, options) {
        if (!model || !model.geometry || !model.material) {
          throw new Error('makeInstanced needs a loaded model');
        }
        var settings = options || {};
        var size = Math.max(0, Math.floor(Number(count) || 0));
        var mesh = new THREE.InstancedMesh(model.geometry, model.material, size);
        mesh.name = model.name;
        var shadows = settings.shadows === undefined ? true : !!settings.shadows;
        mesh.castShadow = shadows;
        mesh.receiveShadow = shadows;
        mesh.frustumCulled = false;
        var tintable = settings.tintable === undefined ? model.tintable === true : !!settings.tintable;
        if (tintable) {
          mesh.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(size * 3).fill(1), 3);
        }
        mesh.count = size;
        if (settings.dynamic === true && mesh.instanceMatrix) {
          mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
        }
        mesh.userData.model = model;
        return mesh;
      }

      var assets = {
        models: models,
        byCategory: byCategory,
        pick: pick,
        makeInstanced: makeInstanced
      };
      reportProgress(context, { phase: 'done', models: models.size, packs: context.packCount || 0 });
      return assets;
    });
  }

  function load(options) {
    var settings = options || {};
    var url =
      typeof settings.url === 'string' && settings.url.length ? settings.url : DEFAULT_MANIFEST_URL;
    var context = {
      tier: settings.tier === 'street' ? 'street' : 'district',
      onProgress: typeof settings.onProgress === 'function' ? settings.onProgress : null,
      controller: typeof AbortController === 'function' ? new AbortController() : null
    };
    var timeoutMs =
      typeof settings.timeoutMs === 'number' && isFinite(settings.timeoutMs) && settings.timeoutMs > 0
        ? settings.timeoutMs
        : DEFAULT_TIMEOUT_MS;
    var settled = false;

    return new Promise(function (resolve) {
      var timer = setTimeout(function () {
        if (settled) return;
        settled = true;
        if (context.controller) {
          try {
            context.controller.abort();
          } catch (error) {
            // abort is best effort
          }
        }
        warnOnce('timed out after ' + timeoutMs + ' ms for ' + url);
        resolve(null);
      }, timeoutMs);

      runLoad(url, context).then(
        function (assets) {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          resolve(assets);
        },
        function (error) {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          warnOnce((error && error.message ? error.message : String(error)) + ' [' + url + ']');
          resolve(null);
        }
      );
    });
  }

  return { load: load };
}

// Registers the offline cache worker. Skips file:// and browsers without
// service workers, and never throws. Returns true when a registration started.
export function registerOfflineCache() {
  try {
    if (typeof navigator === 'undefined' || !navigator.serviceWorker) return false;
    if (typeof location === 'undefined' || !location || !location.protocol) return false;
    var protocol = String(location.protocol);
    if (protocol !== 'http:' && protocol !== 'https:') return false;
    var registration = navigator.serviceWorker.register('assets/sw.js', { scope: './' });
    if (registration && typeof registration.catch === 'function') registration.catch(function () {});
    return true;
  } catch (error) {
    return false;
  }
}
