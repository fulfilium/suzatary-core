// Files a file field was given, read whole (`node.rs`'s `Picked`). A photo is redrawn on a canvas before a byte leaves
// the device, so its metadata (location included) is gone; the field is emptied so the same file can be given again.

export namespace files {
  const LONGEST = 2000
  const QUALITY = 0.85

  // Unchanged when it fits, else scaled so its longest edge is `longest`, never below one pixel.
  export const fitWithin = (width: number, height: number, longest: number = LONGEST): { width: number; height: number } => {
    const edge = Math.max(width, height)
    if (edge <= longest) return { width, height }
    const scale = longest / edge
    return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) }
  }

  const encoded = async (file: Blob): Promise<string> => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    let binary = ""
    for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000))
    return btoa(binary)
  }

  const redrawn = async (file: Blob): Promise<string> => {
    const bitmap = await createImageBitmap(file, { imageOrientation: "from-image" })
    const { width, height } = fitWithin(bitmap.width, bitmap.height)
    const canvas = document.createElement("canvas")
    canvas.width = width
    canvas.height = height
    const context = canvas.getContext("2d")
    if (context === null) throw new Error("this browser cannot draw the photo")
    context.drawImage(bitmap, 0, 0, width, height)
    bitmap.close()
    const blob = await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(made => (made === null ? reject(new Error("the photo could not be encoded")) : resolve(made)), "image/jpeg", QUALITY),
    )
    return encoded(blob)
  }

  // One line per file: media type, base64 and name split by tabs; a file that cannot be read is left out.
  export const read = async (input: HTMLInputElement): Promise<string> => {
    const given = [...(input.files ?? [])]
    input.value = ""
    const lines = await Promise.all(
      given.map(async f => {
        const photo = f.type.startsWith("image/")
        try {
          const base64 = photo ? await redrawn(f) : await encoded(f)
          return `${photo ? "image/jpeg" : f.type}\t${base64}\t${photo ? "" : f.name.replace(/[\r\n]/g, " ")}`
        } catch {
          return null
        }
      }),
    )
    return lines.filter(l => l !== null).join("\n")
  }
}
export default files
