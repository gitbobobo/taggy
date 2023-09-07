use crate::tag::{Tag, TagType};
use crate::taggy_file::TaggyFile;
use crate::utils::lofty_froms::*;
use anyhow::anyhow;
use lofty::{BoundTaggedFile, ParseOptions, Probe, TaggedFile, TaggedFileExt};
use std::fs::OpenOptions;

/// Read all audio tags from the file at given `path`.
pub fn read_all(path: String) -> anyhow::Result<TaggyFile> {
    let tagged = get_tagged_file(path.as_ref())?;
    Ok(taggy_from_tagged(&tagged, &path))
}

/// Read only the primary audio tag from the file at given `path`.
///
/// **Note**: If the primary tag does not exist,
/// this will return a [TaggyFile] with no tags.
///
/// Throws an **exception** when:
/// - path doesn't exists
pub fn read_primary(path: String) -> anyhow::Result<TaggyFile> {
    let tagged = get_tagged_file(path.as_ref())?;

    Ok(TaggyFile {
        tags: get_primary_tag_from_tagged_file(&tagged),
        ..taggy_from_tagged(&tagged, &path)
    })
}

/// Read any audio tag from the file at the given `path`.
///
/// **Note**: If the file has no tags,
/// this will return a [TaggyFile] with an empty tags.
///
/// Throws an **exception** when:
/// - path doesn't exists
pub fn read_any(path: String) -> anyhow::Result<TaggyFile> {
    let tagged = get_tagged_file(path.as_ref())?;

    Ok(TaggyFile {
        tags: get_any_tag_from_tagged_file(&tagged),
        ..taggy_from_tagged(&tagged, &path)
    })
}

/// A helper function to get a [`TaggedFile`] from the given path.
/// the returned file will be used for reading properties only.
fn get_tagged_file(path: &str) -> anyhow::Result<TaggedFile> {
    match Probe::open(path) {
        Err(_) => Err(anyhow!("The file path does not exist!")),
        Ok(file) => match file.read() {
            Ok(tf) => Ok(tf),
            Err(e) => Err(anyhow!(e)),
        },
    }
}

/// Write all provided `tags` to the file at given `path`.
///
/// when `override_existent` is set to `true`, this will remove all existing tags.
/// Otherwise, it will add or update any existing ones.
///
/// Throws an **exception** when:
/// - path doesn't exists
pub fn write_all(
    path: String,
    tags: Vec<Tag>,
    override_existent: bool,
) -> anyhow::Result<TaggyFile> {
    let mut tagged_file = get_bound_tagged_file(&path)?;

    if override_existent {
        tagged_file.clear();
    }

    // convert the provided tags to lofty's Tag
    let lofty_tags = tags.iter().map(|t| t.to_lofty()).collect();

    // add tags to file
    insert_tags(&mut tagged_file, &lofty_tags);

    match tagged_file.save() {
        Ok(_) => Ok(taggy_from_bound_tagged(&tagged_file, &path)),
        Err(e) => Err(anyhow!(e)),
    }
}

fn insert_tags(file: &mut BoundTaggedFile, tags: &Vec<lofty::Tag>) {
    for tag in tags {
        if file.insert_tag(tag.clone()).is_none() {
            eprintln!(
                "The tag type '{:?}' is not supported for the file type '{:?}'",
                tag.tag_type(),
                file.file_type()
            );
        }
    }
}

/// Write the provided `tag` as the primary tag for the file at given `path`.
///
/// If `keep_others` is set to `false`, this will remove any existing tags from the file.
///
/// **Note**: the `tag_type` of the give tag will be overridden with the file primary tag type,
/// so you can set it to any or use [TagType.FilePrimaryType].
///
/// Throws an **exception** when:
/// - path doesn't exists
pub fn write_primary(path: String, tag: Tag, keep_others: bool) -> anyhow::Result<TaggyFile> {
    let mut tagged_file = get_bound_tagged_file(&path)?;

    if !keep_others {
        tagged_file.clear();
    }

    let lofty_tag_type = tagged_file.file_type().primary_tag_type();

    // override the tag's type with the file's primary tag type
    let updated_tag = Tag {
        tag_type: TagType::from(lofty_tag_type),
        ..tag
    };

    // add tags to file
    tagged_file.insert_tag(updated_tag.to_lofty());
    tagged_file.save()?;

    Ok(taggy_from_bound_tagged(&tagged_file, &path))
}

/// Delete all tags from file at given `path`.
///
/// Throws an **exception** when:
/// - path doesn't exists
pub fn remove_all(path: String) -> anyhow::Result<()> {
    let mut tagged = get_bound_tagged_file(&path)?;
    &tagged.clear();
    let tag_type = tagged.primary_tag_type();
    insert_empty_tag(&mut tagged, TagType::from(tag_type));
    tagged
        .save()
        .map_err(|_| anyhow!("Failed to remove file tags"))
}

/// Delete the provided `tag` from file at the given `path`.
///
/// If the `tag` doesn't exist, **no** errors will be returned.
///
/// Throws an **exception** when:
/// - path doesn't exists
pub fn remove_tag(path: String, tag: Tag) -> anyhow::Result<()> {
    let mut tagged = get_bound_tagged_file(&path)?;
    if tagged.remove(tag.tag_type.into()).is_none() {
        // There's no need for saving the file cuz it wasn't changed
        // since the tag doesn't exist
        return Ok(());
    }
    insert_empty_tag(&mut tagged, tag.tag_type);
    tagged.save().map_err(|e| anyhow!(e))
}

/// Adds a tag with empty data to the file.
///
/// This is required to remove the metadata of existing tag with given `tag_type`.
/// The Reason is still unknown to me but it was found during testing.
fn insert_empty_tag(file: &mut BoundTaggedFile, tag_type: TagType) {
    let empty_tag = Tag::new(tag_type);
    file.insert_tag(empty_tag.to_lofty());
}

/// A helper function to get a [`BoundTaggedFile`] from the given path
/// which can be used to read an write tags to the file on disk directly.
fn get_bound_tagged_file(path: &String) -> anyhow::Result<BoundTaggedFile> {
    // We'll need to open our file for reading *and* writing
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.clone())?;

    match BoundTaggedFile::read_from(file, ParseOptions::new()) {
        Ok(file) => Ok(file),
        Err(e) => Err(anyhow!(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::{MimeType, Picture, PictureType};
    use rand::Rng;
    use std::env;
    use std::fs::{copy, remove_file};
    use std::path::Path;

    #[test]
    fn reading_non_existing_file_is_an_error() {
        let result = read_all(get_fake_path());
        assert!(result.is_err());
    }

    #[test]
    fn reading_existing_file_is_ok() {
        let result = read_all(get_audio_sample_file_path());
        assert!(result.is_ok());
    }

    #[test]
    fn reading_file_with_no_tags_should_return_with_empty_tags() {
        let path = get_no_tags_sample_file_path();
        let taggy = read_all(path).unwrap();
        assert!(taggy.tags.is_empty());
    }

    #[test]
    fn writing_tags_to_non_existing_file_is_an_error() {
        let result = write_all(get_fake_path(), vec![], true);
        assert!(result.is_err());
    }

    #[test]
    fn it_updates_file_primary_tag() {
        let path = get_audio_sample_file_path();
        let old_tag = read_primary(path.clone()).unwrap().first_tag().unwrap();
        let new_tag = Tag::builder().with_tag_type(old_tag.tag_type).create();
        let result = write_primary(path.clone(), new_tag.clone(), false);
        assert!(result.is_ok());
        let tag_after_write = read_primary(path.clone()).unwrap().first_tag().unwrap();
        assert_eq!(tag_after_write, new_tag);
    }

    #[test]
    fn it_updates_a_file_with_no_tags_after_writing() {
        let path = duplicate_file(get_no_tags_sample_file_path());
        // `TagType::Id3v2` is the primary tag type for the file so the equality assertion passes for
        // this property.
        let tag = Tag::builder().with_tag_type(TagType::Id3v2).create();
        // act
        let result = write_primary(path.clone(), tag.clone(), true);
        // clean-up
        remove_file(Path::new(&path)).expect("Failed to clean up test asset");
        // assert
        let created_tag = result.unwrap().tags.first().unwrap().clone();
        assert_eq!(created_tag, tag);
    }

    #[test]
    fn it_adds_image_to_tag() {
        let path = duplicate_file(get_no_tags_sample_file_path());
        let pic = get_pic_from_asset();
        let tag = Tag::builder().with_pictures(vec![pic.clone()]).create();
        // act
        let taggy =
            write_primary(path.clone(), tag.clone(), false).expect("Failed to write primary tag");
        let tag = taggy.primary_tag().unwrap();
        let added_picture = tag.pictures.first().unwrap();
        // clean up
        remove_file(Path::new(&path)).expect("Failed to clean up test asset");
        // assert
        assert_eq!(added_picture.pic_data, pic.pic_data);
    }

    #[test]
    fn it_removes_tag_from_file() {
        let path = duplicate_file(get_audio_sample_file_path());
        let taggy = read_primary(path.clone()).expect("Failed to read primary tag");
        let tag = taggy.primary_tag();
        // first assert a tag exists
        assert!(tag.as_ref().is_some());
        // act
        let remove_result = remove_tag(path.clone(), tag.unwrap());
        // if remove_result.
        // assert
        assert!(remove_result.is_ok());
        let taggy_after = read_primary(path.clone()).expect("Failed to read primary tag");
        assert!(taggy_after.primary_tag().is_none());
        // clean_up
        remove_file(path.clone()).expect("Failed to remove test asset");
    }
    /*
     * Helper Functions
     */
    fn get_pic_from_asset() -> Picture {
        let bytes = std::fs::read(get_image_path()).expect("Failed to read image bytes");
        Picture {
            pic_type: PictureType::CoverFront,
            pic_data: bytes,
            mime_type: Some(MimeType::Jpeg),
            width: None,
            height: None,
            color_depth: None,
            num_colors: None,
        }
    }

    /// Creates a copy of the file at given `path` and place it in the same directory.
    /// This returns the path of created copy.
    ///
    /// ## Note:
    /// When finished, be sure to call ['std::fs::remove_file(path_of_copy)']
    fn duplicate_file(path: String) -> String {
        let rand = rand::thread_rng().gen_range(0..=100);
        let file_name = format!("test_copy_{:?}.mp3", rand);
        let path = Path::new(&path);
        let copy_path = path.with_file_name(file_name);
        let _ = copy(path, copy_path.as_path());
        copy_path.to_str().unwrap().to_string()
    }

    fn get_image_path() -> String {
        let current_dir = env::current_dir().expect("Failed to get current directory");
        current_dir
            .join("test_samples\\image.jpg")
            .to_str()
            .unwrap()
            .to_string()
    }

    fn get_audio_sample_file_path() -> String {
        let current_dir = env::current_dir().expect("Failed to get current directory");
        current_dir
            .parent()
            .unwrap()
            .join("test_samples\\sample.mp3")
            .to_str()
            .unwrap()
            .to_string()
    }

    fn get_no_tags_sample_file_path() -> String {
        let current_dir = env::current_dir().expect("Failed to get current directory");
        current_dir
            .join("test_samples\\no_tags.mp3")
            .to_str()
            .unwrap()
            .to_string()
    }

    fn get_fake_path() -> String {
        String::from("fake/path/file.mp3")
    }
}
