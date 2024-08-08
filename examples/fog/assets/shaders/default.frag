uniform vec2 viewportSize;
uniform sampler2D renderTexture;

out vec4 color;

// --------------------------------------------------------- //
// Main entry point
// --------------------------------------------------------- //
void main()
{
    vec2 texCoords = gl_FragCoord.xy / viewportSize.xy;
    //texCoords = vec2(0.5, 0.5);
    vec3 renderTextureColor = texture(renderTexture, texCoords.xy).rgb;

    color = vec4(renderTextureColor, 1.0);
    //color = vec4(1.0, 0.0, 0.0, 1.0);
}