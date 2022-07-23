use azalea_buf::{McBufReadable, McBufVarReadable, McBufWritable, Readable, Writable};
use std::io::{Read, Write};

use crate::BitStorage;

#[derive(Clone, Debug, Copy)]
pub enum PalettedContainerType {
    Biomes,
    BlockStates,
}

#[derive(Clone, Debug)]
pub struct PalettedContainer {
    pub bits_per_entry: u8,
    pub palette: Palette,
    /// Compacted list of indices pointing to entry IDs in the Palette.
    pub storage: BitStorage,
    pub container_type: PalettedContainerType,
}

impl PalettedContainer {
    pub fn new(container_type: &'static PalettedContainerType) -> Result<Self, String> {
        let palette = Palette::SingleValue(0);
        let size = container_type.size();
        let storage = BitStorage::new(0, size, Some(vec![])).unwrap();

        Ok(PalettedContainer {
            bits_per_entry: 0,
            palette,
            storage,
            container_type: *container_type,
        })
    }

    pub fn read_with_type(
        buf: &mut impl Read,
        container_type: &'static PalettedContainerType,
    ) -> Result<Self, String> {
        let bits_per_entry = buf.read_byte()?;
        let palette_type = PaletteType::from_bits_and_type(bits_per_entry, container_type);
        let palette = palette_type.read(buf)?;
        let size = container_type.size();

        let data = Vec::<u64>::read_from(buf)?;
        debug_assert!(
            bits_per_entry != 0 || data.is_empty(),
            "Bits per entry is 0 but data is not empty."
        );
        let storage = BitStorage::new(bits_per_entry.into(), size, Some(data)).unwrap();

        Ok(PalettedContainer {
            bits_per_entry,
            palette,
            storage,
            container_type: *container_type,
        })
    }

    pub fn get_index(&self, x: usize, y: usize, z: usize) -> usize {
        let size_bits = self.container_type.size_bits();

        (((y << size_bits) | z) << size_bits) | x
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        let paletted_value = self.storage.get(self.get_index(x, y, z));
        println!("palette: {:?}", self.palette);
        self.palette.value_for(paletted_value as usize)
    }

    /// Sets the id at the given coordinates and return the previous id
    pub fn get_and_set(&mut self, x: usize, y: usize, z: usize, value: u32) -> u32 {
        let paletted_value = self.id_for(value);
        self.storage
            .get_and_set(self.get_index(x, y, z), paletted_value as u64) as u32
    }

    /// Sets the id at the given coordinates and return the previous id
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: u32) {
        let palette = self.palette.clone();
        let paletted_value = self.id_for(value);
        self.storage
            .set(self.get_index(x, y, z), paletted_value as u64)
    }

    fn create_or_reuse_data(&self, bits_per_entry: u8) -> PalettedContainer {
        println!("create_or_reuse_data {:?}", bits_per_entry);
        let new_palette_type =
            PaletteType::from_bits_and_type(bits_per_entry, &self.container_type);
        let old_palette_type: PaletteType = (&self.palette).into();
        if new_palette_type == old_palette_type {
            return self.clone();
        }
        println!(
            "self.container_type.size(): {:?}",
            self.container_type.size()
        );
        let storage =
            BitStorage::new(bits_per_entry as usize, self.container_type.size(), None).unwrap();

        // sanity check
        debug_assert_eq!(storage.size(), self.container_type.size());

        let palette = new_palette_type.into_empty_palette();
        PalettedContainer {
            bits_per_entry,
            palette,
            storage,
            container_type: self.container_type,
        }
    }

    fn on_resize(&mut self, bits_per_entry: u8, value: u32) -> usize {
        println!("resizing! {} {}", bits_per_entry, value);
        let mut new_data = self.create_or_reuse_data(bits_per_entry);
        new_data.copy_from(&self.palette, &self.storage);
        println!("copied to new data: {:?}", new_data);
        *self = new_data;
        self.id_for(value)
    }

    fn copy_from(&mut self, palette: &Palette, storage: &BitStorage) {
        // println!("old_palette: {:?}", old_palette);
        // println!("palette: {:?}", palette);
        for i in 0..storage.size() {
            let value = palette.value_for(storage.get(i) as usize);
            let id = self.id_for(value) as u64;
            println!("index: {}, storage size {}", i, self.storage.size());
            self.storage.set(i, id);
        }
    }

    pub fn id_for(&mut self, value: u32) -> usize {
        match &mut self.palette {
            Palette::SingleValue(v) => {
                if *v != value {
                    self.on_resize(1, value)
                } else {
                    0
                }
            }
            Palette::Linear(palette) => {
                println!("finding {:?} in linear palette: {:?}", value, palette);
                if let Some(index) = palette.iter().position(|v| *v == value) {
                    return index as usize;
                }
                let capacity = 2usize.pow(self.bits_per_entry.into());
                println!("capacity: {}, palette.len: {}", capacity, palette.len());
                if capacity > palette.len() {
                    palette.push(value);
                    println!("palette: {:?}", palette);
                    palette.len() - 1
                } else {
                    // vanilla uses LinearPalette.bits but i think this is the same
                    self.on_resize(self.bits_per_entry + 1, value)
                }
            }
            Palette::Hashmap(palette) => {
                // TODO? vanilla keeps this in memory as a hashmap, but also i don't care
                if let Some(index) = palette.iter().position(|v| *v == value) {
                    return index as usize;
                }
                // vanilla uses LinearPalette.bits but i think this is the same
                self.on_resize(self.bits_per_entry + 1, value)
            }
            Palette::Global => value as usize,
        }
    }
}

impl McBufWritable for PalettedContainer {
    fn write_into(&self, buf: &mut impl Write) -> Result<(), std::io::Error> {
        buf.write_byte(self.bits_per_entry)?;
        self.palette.write_into(buf)?;
        self.storage.data.write_into(buf)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteType {
    SingleValue,
    Linear,
    Hashmap,
    Global,
}

#[derive(Clone, Debug)]
pub enum Palette {
    /// ID of the corresponding entry in its global palette
    SingleValue(u32),
    // in vanilla this keeps a `size` field that might be less than the length, but i'm not sure it's actually needed?
    Linear(Vec<u32>),
    Hashmap(Vec<u32>),
    Global,
}

impl Palette {
    pub fn value_for(&self, id: usize) -> u32 {
        match self {
            Palette::SingleValue(v) => *v,
            Palette::Linear(v) => v[id],
            Palette::Hashmap(v) => v[id],
            Palette::Global => id as u32,
        }
    }
}

impl McBufWritable for Palette {
    fn write_into(&self, buf: &mut impl Write) -> Result<(), std::io::Error> {
        match self {
            Palette::SingleValue(value) => {
                value.write_into(buf)?;
            }
            Palette::Linear(values) => {
                values.write_into(buf)?;
            }
            Palette::Hashmap(values) => {
                values.write_into(buf)?;
            }
            Palette::Global => {}
        }
        Ok(())
    }
}

impl PaletteType {
    pub fn from_bits_and_type(bits_per_entry: u8, container_type: &PalettedContainerType) -> Self {
        match container_type {
            PalettedContainerType::BlockStates => match bits_per_entry {
                0 => PaletteType::SingleValue,
                1..=4 => PaletteType::Linear,
                5..=8 => PaletteType::Hashmap,
                _ => PaletteType::Global,
            },
            PalettedContainerType::Biomes => match bits_per_entry {
                0 => PaletteType::SingleValue,
                1..=3 => PaletteType::Linear,
                _ => PaletteType::Global,
            },
        }
    }

    pub fn read(&self, buf: &mut impl Read) -> Result<Palette, String> {
        Ok(match self {
            PaletteType::SingleValue => Palette::SingleValue(u32::var_read_from(buf)?),
            PaletteType::Linear => Palette::Linear(Vec::<u32>::var_read_from(buf)?),
            PaletteType::Hashmap => Palette::Hashmap(Vec::<u32>::var_read_from(buf)?),
            PaletteType::Global => Palette::Global,
        })
    }

    pub fn into_empty_palette(&self) -> Palette {
        match self {
            PaletteType::SingleValue => Palette::SingleValue(0),
            PaletteType::Linear => Palette::Linear(Vec::new()),
            PaletteType::Hashmap => Palette::Hashmap(Vec::new()),
            PaletteType::Global => Palette::Global,
        }
    }
}

impl From<&Palette> for PaletteType {
    fn from(palette: &Palette) -> Self {
        match palette {
            Palette::SingleValue(_) => PaletteType::SingleValue,
            Palette::Linear(_) => PaletteType::Linear,
            Palette::Hashmap(_) => PaletteType::Hashmap,
            Palette::Global => PaletteType::Global,
        }
    }
}

impl PalettedContainerType {
    fn size_bits(&self) -> usize {
        match self {
            PalettedContainerType::BlockStates => 4,
            PalettedContainerType::Biomes => 2,
        }
    }

    fn size(&self) -> usize {
        1 << self.size_bits() * 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_resizing() {
        let mut palette_container =
            PalettedContainer::new(&PalettedContainerType::BlockStates).unwrap();

        assert_eq!(palette_container.bits_per_entry, 0);
        assert_eq!(palette_container.get(8, 8, 8), 0);
        assert_eq!(
            PaletteType::from(&palette_container.palette),
            PaletteType::SingleValue
        );
        palette_container.set(8, 8, 8, 1);
        assert_eq!(palette_container.get(8, 8, 8), 1);
        assert_eq!(
            PaletteType::from(&palette_container.palette),
            PaletteType::Linear
        );
    }
}
