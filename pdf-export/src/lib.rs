use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

pub struct Ctx {
    doc: Document,
}

#[repr(C)]
pub enum Error {
    Ok = 0,
    BadContext,
    BadPath,
    PageNotFound,
    NoPageObject,
    NoMediaBox,
    NotEnoughMediaBoxPoints,
    FailedToWriteFile,
}

impl From<Result<(), Error>> for Error {
    fn from(value: Result<(), Error>) -> Self {
        match value {
            Ok(()) => Self::Ok,
            Err(e) => e,
        }
    }
}

/// Creates a new export context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pdf_export_ctx_new(path: *const u8, path_len: usize) -> *mut Ctx {
    assert!(!path.is_null());
    let path = unsafe { std::slice::from_raw_parts(path, path_len) };
    let Ok(path) = std::str::from_utf8(path) else {
        return std::ptr::null_mut();
    };
    let Some(c) = Ctx::new(path) else {
        return std::ptr::null_mut();
    };

    Box::leak(c) as *mut Ctx
}

/// Adds an image to a page.
///
/// Regardless of the dimensions of the image, it's stretched on the page.
/// The image data is assumed to come from Cairo's ARGB32 format (i.e. ARGB in native endian).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pdf_export_ctx_add_image(
    ctx: *mut Ctx,
    page: usize,
    data: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) -> Error {
    assert!(!ctx.is_null());
    let ctx = unsafe { ctx.as_mut() };
    let Some(ctx) = ctx else {
        return Error::BadContext;
    };
    assert!(!data.is_null());
    let data = unsafe { std::slice::from_raw_parts(data, data_len) };
    ctx.add_image(page, data, width, height, (x1, y1), (x2, y2))
        .into()
}

/// Reads the size of a page in user space units (1/72 inch).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pdf_export_ctx_page_size(
    ctx: *mut Ctx,
    page: usize,
    width: *mut f32,
    height: *mut f32,
) -> Error {
    assert!(!ctx.is_null());
    let ctx = unsafe { ctx.as_mut() };
    let Some(ctx) = ctx else {
        return Error::BadContext;
    };
    assert!(!width.is_null() && !height.is_null());
    match ctx.page_size(page) {
        Ok((w, h)) => {
            unsafe {
                *width = w;
                *height = h;
            }
            Error::Ok
        }
        Err(e) => e,
    }
}

/// Saves the annotated PDF in some path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pdf_export_ctx_save(
    ctx: *mut Ctx,
    path: *const u8,
    path_len: usize,
) -> Error {
    assert!(!ctx.is_null());
    let ctx = unsafe { ctx.as_mut() };
    let Some(ctx) = ctx else {
        return Error::BadContext;
    };
    assert!(!path.is_null());
    let path = unsafe { std::slice::from_raw_parts(path, path_len) };
    let Ok(path) = std::str::from_utf8(path) else {
        return Error::BadPath;
    };
    ctx.save(path).into()
}

/// Destroys the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pdf_export_ctx_free(ctx: *mut Ctx) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(ctx);
    }
}

impl Ctx {
    pub fn new(path: &str) -> Option<Box<Self>> {
        Document::load(path).map(|doc| Box::new(Self { doc })).ok()
    }

    pub fn add_image(
        &mut self,
        page: usize,
        data: &[u8],
        width: u32,
        height: u32,
        tl: (f32, f32),
        br: (f32, f32),
    ) -> Result<(), Error> {
        if data.is_empty() {
            // nothing wrong - we don't need to insert anything
            return Ok(());
        }

        let mask_stream = make_mask_stream(data, width, height);
        let mask_id = self.doc.add_object(mask_stream);
        let image_stream = make_image_stream(data, width, height, mask_id);

        let Some(page_id) = self.doc.page_iter().nth(page) else {
            return Err(Error::NoPageObject);
        };
        let size = self.page_size_for_id(page_id)?;

        let _ = self.doc.insert_image(
            page_id,
            image_stream,
            (tl.0 * size.0, (1.0 - br.1) * size.1),
            ((br.0 - tl.0) * size.0, (br.1 - tl.1) * size.1),
        );
        Ok(())
    }

    pub fn save(&mut self, path: &str) -> Result<(), Error> {
        self.doc.compress();
        self.doc
            .save(path)
            .inspect_err(|e| eprintln!("Failed to save to {path}: {e}"))
            .map_err(|_| Error::FailedToWriteFile)?;
        Ok(())
    }

    pub fn page_size(&self, page: usize) -> Result<(f32, f32), Error> {
        let Some(page_id) = self.doc.page_iter().nth(page) else {
            return Err(Error::NoPageObject);
        };
        self.page_size_for_id(page_id)
    }

    pub fn page_size_for_id(&self, page_id: ObjectId) -> Result<(f32, f32), Error> {
        let [ll_x, ll_y, ur_x, ur_y] = self.page_rect(page_id)?;
        // 7.9.5 Rectangles:
        //   Although rectangles are conventionally specified by their lower-left
        //   and upper-right corners, it is acceptable to specify any two diagonally
        //   opposite corners.
        Ok(((ur_x - ll_x).abs(), (ur_y - ll_y).abs()))
    }

    fn page_rect(&self, page_id: ObjectId) -> Result<[f32; 4], Error> {
        let Ok(Object::Dictionary(page_obj)) = self.doc.get_object(page_id) else {
            return Err(Error::NoPageObject);
        };
        let Ok(Object::Array(media_box)) = page_obj.get(b"MediaBox") else {
            return Err(Error::NoMediaBox);
        };

        let mut rect = [0.0; 4];
        let mut iter = media_box.iter().flat_map(|it| match it {
            Object::Real(r) => Some(*r),
            Object::Integer(i) => Some(*i as f32),
            _ => None,
        });
        for i in 0..rect.len() {
            rect[i] = iter.next().ok_or(Error::NotEnoughMediaBoxPoints)?;
        }
        Ok(rect)
    }
}

fn make_mask_stream(data: &[u8], width: u32, height: u32) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", width);
    dict.set("Height", height);
    dict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
    dict.set("BitsPerComponent", 8);

    assert!(data.len() % 4 == 0);
    let mut alpha = Vec::with_capacity(data.len() / 4);
    for i in 0..data.len() / 4 {
        alpha.push(data[i * 4 + 3]);
    }

    let mut s = Stream::new(dict, alpha);
    let _ = s.compress();
    s
}

fn make_image_stream(data: &[u8], width: u32, height: u32, mask_ref: ObjectId) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", width);
    dict.set("Height", height);
    dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
    dict.set("BitsPerComponent", 8);
    dict.set("SMask", Object::Reference(mask_ref));

    assert!(data.len() % 4 == 0);
    let mut rgb = Vec::with_capacity((data.len() / 4) * 3);
    for i in 0..data.len() / 4 {
        // Cairo saves ARGB in native endian -> little endian for x86 and ARM
        rgb.push(data[i * 4 + 2]); // R
        rgb.push(data[i * 4 + 1]); // G
        rgb.push(data[i * 4]); // B
    }

    let mut s = Stream::new(dict, rgb);
    let _ = s.compress();
    s
}
